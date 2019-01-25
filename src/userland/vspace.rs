use core::marker::PhantomData;
use core::ops::{Add, Sub};
use crate::pow::Pow;
use crate::userland::{
    paging, role, ASIDPool, AssignedPageDirectory, BootInfo, Cap, CapRights, LocalCNode, LocalCap,
    MappedPage, MappedPageTable, PhantomCap, SeL4Error, UnassignedPageDirectory, UnmappedPage,
    UnmappedPageTable, Untyped,
};
use generic_array::sequence::Concat;
use generic_array::{arr, arr_impl, ArrayLength, GenericArray};
use sel4_sys::*;
use typenum::operator_aliases::{Diff, Sub1, Sum};
use typenum::{Unsigned, B1, U0, U1, U10, U128, U14, U16, U2, U256};

// encapsulate vspace setup
pub struct VSpace<
    PageDirFreeSlots: Unsigned,
    PageTableFreeSlots: Unsigned,
    FilledPageTableCount: Unsigned,
> where
    FilledPageTableCount: ArrayLength<LocalCap<MappedPageTable<U0>>>,
{
    page_dir: LocalCap<AssignedPageDirectory<PageDirFreeSlots>>,
    current_page_table: LocalCap<MappedPageTable<PageTableFreeSlots>>,
    filled_page_tables: GenericArray<LocalCap<MappedPageTable<U0>>, FilledPageTableCount>,
}

impl<PageDirFreeSlots: Unsigned, PageTableFreeSlots: Unsigned, FilledPageTableCount: Unsigned>
    VSpace<PageDirFreeSlots, PageTableFreeSlots, FilledPageTableCount>
where
    FilledPageTableCount: ArrayLength<LocalCap<MappedPageTable<U0>>>,
{
    // Set up the barest minimal vspace; it will be further initialized to be
    // actually useful in the 'new' constructor. This needs untyped caps for the
    // page dir and page table storage, two cnode slots to retype them with, and
    // will consume 1 ASID to assign the page dir and 1 slot from the page dir
    // to map in the initial page table.
    fn internal_new<CNodeFreeSlots: Unsigned>(
        // TODO: model ASIDPool capacity at the type level
        asid_pool: &mut LocalCap<ASIDPool>,
        page_dir_ut: LocalCap<Untyped<U14>>,
        page_table_ut: LocalCap<Untyped<U10>>,
        dest_cnode: LocalCap<LocalCNode<CNodeFreeSlots>>,
    ) -> Result<
        (
            VSpace<
                Sub1<paging::BasePageDirFreeSlots>,
                paging::BasePageTableFreeSlots,
                U0, // FilledPageTableCount
            >,
            // dest_cnode
            LocalCap<LocalCNode<Diff<CNodeFreeSlots, U2>>>,
        ),
        SeL4Error,
    >
    where
        CNodeFreeSlots: Sub<U2>,
        Diff<CNodeFreeSlots, U2>: Unsigned,
    {
        let (cnode, dest_cnode) = dest_cnode.reserve_region::<U2>();

        // allocate the page dir and initial page table
        let (page_dir, cnode): (LocalCap<UnassignedPageDirectory>, _) =
            page_dir_ut.retype_local(cnode)?;

        let (initial_page_table, _cnode): (LocalCap<UnmappedPageTable>, _) =
            page_table_ut.retype_local(cnode)?;

        // assign the page dir and map in the initial page table.
        let page_dir = asid_pool.assign_minimal(page_dir)?;
        let (initial_page_table, page_dir) = page_dir.map_page_table(initial_page_table)?;

        Ok((
            VSpace {
                page_dir,
                current_page_table: initial_page_table,
                filled_page_tables: arr![LocalCap<MappedPageTable<U0>>;],
            },
            dest_cnode,
        ))
    }

    pub fn new<CNodeFreeSlots: Unsigned>(
        boot_info: &mut BootInfo<PageDirFreeSlots>,
        ut16: LocalCap<Untyped<U16>>,
        dest_cnode: LocalCap<LocalCNode<CNodeFreeSlots>>,
    ) -> Result<
        (
            VSpace<
                Diff<paging::BasePageDirFreeSlots, U2>,
                paging::BasePageTableFreeSlots,
                U1, // FilledPageTableCount
            >,
            // dest_cnode
            LocalCap<LocalCNode<Diff<CNodeFreeSlots, U256>>>,
        ),
        SeL4Error,
    >
    where
        CNodeFreeSlots: Sub<U256>,
        Diff<CNodeFreeSlots, U256>: Unsigned,
    {
        let (cnode, dest_cnode) = dest_cnode.reserve_region::<U256>();

        let (ut14, page_dir_ut, _, _, cnode) = ut16.quarter(cnode)?;
        let (ut12, _, _, _, cnode) = ut14.quarter(cnode)?;
        let (ut10, initial_page_table_ut, second_page_table_ut, _, cnode) = ut12.quarter(cnode)?;
        let (ut8, _, _, _, cnode) = ut10.quarter(cnode)?;
        let (ut6, _, _, _, cnode) = ut8.quarter(cnode)?;
        let (_, _, _, _, cnode) = ut6.quarter(cnode)?;

        let (vspace, cnode) = Self::internal_new(
            &mut boot_info.asid_pool,
            page_dir_ut,
            initial_page_table_ut,
            cnode,
        )?;

        ////////////////////////////////////////
        // Map in the code (user image) pages //
        ////////////////////////////////////////

        // Do this immediately after assigning the page directory because it has
        // to be the first page table; that's the portion of the address space
        // where the program expects to find itself.
        //
        // TODO: This limits us to a megabyte of code. If we want to allow more,
        // we need more than one page table here.
        // let (code_page_table, page_dir) = page_dir.map_page_table(code_page_table)?;

        // munge the code page table so the first page gets mapped to 0x00010000
        // TODO: can we do this as skip_until_addr::<0x10000>() instead?
        let vspace = vspace.skip_pages::<U16>();

        // TODO: the number of pages we reserve here needs to be checked against the
        // size of the binary.
        let (cnode_slot_reservation_iter, cnode) = cnode.reservation_iter::<U128>();
        let (code_page_slot_reservation_iter, vspace) = vspace.page_slot_reservation_iter::<U128>();

        for ((page_cap, slot_cnode), page_slot) in boot_info
            .user_image_pages_iter()
            .zip(cnode_slot_reservation_iter)
            .zip(code_page_slot_reservation_iter)
        {
            let (copied_page_cap, _) = page_cap.copy(&cnode, slot_cnode, CapRights::W)?;
            let _mapped_page = page_slot.map_page(copied_page_cap)?;
        }

        // Let the user start with a fresh page table since we have plenty of
        // unused CNode and Untyped capacity hanging around in here.
        let (vspace, _cnode) = vspace.next_page_table(second_page_table_ut, cnode)?;
        Ok((vspace, dest_cnode))
    }

    // fn map_page...

    pub(super) fn next_page_table<CNodeFreeSlots: Unsigned>(
        self,
        new_page_table_ut: LocalCap<Untyped<U10>>,
        dest_cnode: LocalCap<LocalCNode<CNodeFreeSlots>>,
    ) -> Result<
        (
            VSpace<
                Sub1<PageDirFreeSlots>,
                paging::BasePageTableFreeSlots,
                Sum<FilledPageTableCount, U1>,
            >,
            LocalCap<LocalCNode<Sub1<CNodeFreeSlots>>>,
        ),
        SeL4Error,
    >
    where
        PageDirFreeSlots: Sub<B1>,
        Sub1<PageDirFreeSlots>: Unsigned,

        PageTableFreeSlots: Sub<PageTableFreeSlots, Output = U0>,

        FilledPageTableCount: Add<U1>,
        Sum<FilledPageTableCount, U1>: ArrayLength<LocalCap<MappedPageTable<U0>>>,

        CNodeFreeSlots: Sub<B1>,
        Sub1<CNodeFreeSlots>: Unsigned,
    {
        let (new_page_table, dest_cnode): (LocalCap<UnmappedPageTable>, _) =
            new_page_table_ut.retype_local(dest_cnode)?;
        let (new_page_table, page_dir) = self.page_dir.map_page_table(new_page_table)?;
        let current_page_table = self.current_page_table.skip_remaining_pages();

        Ok((
            VSpace {
                page_dir: page_dir,
                current_page_table: new_page_table,
                filled_page_tables: self
                    .filled_page_tables
                    .concat(arr![LocalCap<MappedPageTable<U0>>;current_page_table]),
            },
            dest_cnode,
        ))
    }

    pub(super) fn skip_pages<Count: Unsigned>(
        self,
    ) -> VSpace<PageDirFreeSlots, Diff<PageTableFreeSlots, Count>, FilledPageTableCount>
    where
        PageTableFreeSlots: Sub<Count>,
        Diff<PageTableFreeSlots, Count>: Unsigned,
    {
        VSpace {
            page_dir: self.page_dir,
            current_page_table: self.current_page_table.skip_pages::<Count>(),
            filled_page_tables: self.filled_page_tables,
        }
    }

    pub(super) fn page_slot_reservation_iter<Count: Unsigned>(
        self,
    ) -> (
        impl Iterator<Item = PageSlot>,
        VSpace<PageDirFreeSlots, Diff<PageTableFreeSlots, Count>, FilledPageTableCount>,
    )
    where
        PageTableFreeSlots: Sub<Count>,
        Diff<PageTableFreeSlots, Count>: Unsigned,
    {
        let start_slot_num = self.current_page_table.cap_data.next_free_slot;
        let page_table_cptr = self.current_page_table.cptr;
        let page_table_vaddr = self.current_page_table.cap_data.vaddr;
        let page_dir_cptr = self.page_dir.cptr;

        let iter = (start_slot_num..start_slot_num + Count::USIZE).map(move |slot_num| PageSlot {
            page_dir: Cap {
                cptr: page_dir_cptr,
                _role: PhantomData,
                cap_data: AssignedPageDirectory {
                    // this is unused, but we have to fill it out.
                    next_free_slot: core::usize::MAX,
                    _free_slots: PhantomData,
                },
            },
            page_table: Cap {
                cptr: page_table_cptr,
                _role: PhantomData,
                cap_data: MappedPageTable {
                    next_free_slot: slot_num,
                    vaddr: page_table_vaddr,
                    _free_slots: PhantomData,
                },
            },
        });

        (iter, self.skip_pages::<Count>())
    }
}

pub struct PageSlot {
    page_table: LocalCap<MappedPageTable<U1>>,
    page_dir: LocalCap<AssignedPageDirectory<U0>>,
}

impl PageSlot {
    pub fn map_page(
        mut self,
        page: LocalCap<UnmappedPage>,
    ) -> Result<LocalCap<MappedPage>, SeL4Error> {
        let (res, _) = self.page_table.map_page(page, &mut self.page_dir)?;
        Ok(res)
    }
}

impl Cap<ASIDPool, role::Local> {
    pub fn assign_minimal(
        &mut self,
        page_dir: LocalCap<UnassignedPageDirectory>,
    ) -> Result<LocalCap<AssignedPageDirectory<paging::BasePageDirFreeSlots>>, SeL4Error> {
        let err = unsafe { seL4_ARM_ASIDPool_Assign(self.cptr, page_dir.cptr) };

        if err != 0 {
            return Err(SeL4Error::ASIDPoolAssign(err));
        }

        let page_dir = Cap {
            cptr: page_dir.cptr,
            _role: PhantomData,
            cap_data: AssignedPageDirectory::<paging::BasePageDirFreeSlots> {
                next_free_slot: 0,
                _free_slots: PhantomData,
            },
        };

        Ok(page_dir)
    }
}

// vspace related capability operations
impl<FreeSlots: Unsigned> LocalCap<AssignedPageDirectory<FreeSlots>> {
    pub fn map_page_table(
        self,
        page_table: LocalCap<UnmappedPageTable>,
    ) -> Result<
        (
            LocalCap<MappedPageTable<Pow<paging::PageTableBits>>>,
            LocalCap<AssignedPageDirectory<Sub1<FreeSlots>>>,
        ),
        SeL4Error,
    >
    where
        FreeSlots: Sub<B1>,
        Sub1<FreeSlots>: Unsigned,
    {
        let page_table_vaddr = self.cap_data.next_free_slot
            << (paging::PageBits::USIZE + paging::PageTableBits::USIZE);

        // map the page table
        let err = unsafe {
            seL4_ARM_PageTable_Map(
                page_table.cptr,
                self.cptr,
                page_table_vaddr,
                seL4_ARM_VMAttributes_seL4_ARM_PageCacheable
                    | seL4_ARM_VMAttributes_seL4_ARM_ParityEnabled, // | seL4_ARM_VMAttributes_seL4_ARM_ExecuteNever
            )
        };

        if err != 0 {
            return Err(SeL4Error::MapPageTable(err));
        }

        Ok((
            // page table
            Cap {
                cptr: page_table.cptr,
                _role: PhantomData,
                cap_data: MappedPageTable {
                    vaddr: page_table_vaddr,
                    next_free_slot: 0,
                    _free_slots: PhantomData,
                },
            },
            // page_dir
            Cap {
                cptr: self.cptr,
                _role: PhantomData,
                cap_data: AssignedPageDirectory {
                    next_free_slot: self.cap_data.next_free_slot + 1,
                    _free_slots: PhantomData,
                },
            },
        ))
    }
}

impl<FreeSlots: Unsigned> LocalCap<MappedPageTable<FreeSlots>> {
    pub fn map_page<PageDirFreeSlots: Unsigned>(
        self,
        page: LocalCap<UnmappedPage>,
        page_dir: &mut LocalCap<AssignedPageDirectory<PageDirFreeSlots>>,
    ) -> Result<
        (
            LocalCap<MappedPage>,
            LocalCap<MappedPageTable<Sub1<FreeSlots>>>,
        ),
        SeL4Error,
    >
    where
        FreeSlots: Sub<B1>,
        Sub1<FreeSlots>: Unsigned,
    {
        let page_vaddr =
            self.cap_data.vaddr + (self.cap_data.next_free_slot << paging::PageBits::USIZE);

        let err = unsafe {
            seL4_ARM_Page_Map(
                page.cptr,
                page_dir.cptr,
                page_vaddr,
                CapRights::RW.into(), // rights
                // TODO: JON! What do we write here? The default (according to
                // sel4_ appears to be pageCachable | parityEnabled)
                seL4_ARM_VMAttributes_seL4_ARM_PageCacheable
                    | seL4_ARM_VMAttributes_seL4_ARM_ParityEnabled
                    // | seL4_ARM_VMAttributes_seL4_ARM_ExecuteNever,
            )
        };
        if err != 0 {
            return Err(SeL4Error::MapPage(err));
        }
        Ok((
            Cap {
                cptr: page.cptr,
                _role: PhantomData,
                cap_data: MappedPage { vaddr: page_vaddr },
            },
            Cap {
                cptr: self.cptr,
                _role: PhantomData,
                cap_data: MappedPageTable {
                    vaddr: self.cap_data.vaddr,
                    next_free_slot: self.cap_data.next_free_slot + 1,
                    _free_slots: PhantomData,
                },
            },
        ))
    }

    pub fn temporarily_map_page<PageDirFreeSlots: Unsigned, F, Out>(
        &mut self,
        unmapped_page: LocalCap<UnmappedPage>,
        mut page_dir: &mut LocalCap<AssignedPageDirectory<PageDirFreeSlots>>,
        f: F,
    ) -> Result<(Out, LocalCap<UnmappedPage>), SeL4Error>
    where
        F: Fn(&LocalCap<MappedPage>) -> Out,
        FreeSlots: Sub<B1>,
        Sub1<FreeSlots>: Unsigned,
    {
        // Make a temporary copy of the cap, so we can build on map_page, which
        // requires a move. This is fine because we're unmapping it at the end,
        // ending up with an effectively unmodified page table.
        let temp_page_table: LocalCap<MappedPageTable<FreeSlots>> = Cap {
            cptr: self.cptr,
            _role: PhantomData,
            cap_data: MappedPageTable {
                next_free_slot: self.cap_data.next_free_slot,
                vaddr: self.cap_data.vaddr,
                _free_slots: PhantomData,
            },
        };

        let (mapped_page, _) = temp_page_table.map_page(unmapped_page, &mut page_dir)?;
        let res = f(&mapped_page);
        let unmapped_page = mapped_page.unmap()?;

        Ok((res, unmapped_page))
    }

    pub fn unmap(self) -> Result<Cap<UnmappedPageTable, role::Local>, SeL4Error> {
        let err = unsafe { seL4_ARM_PageTable_Unmap(self.cptr) };
        if err != 0 {
            return Err(SeL4Error::UnmapPageTable(err));
        }
        Ok(Cap {
            cptr: self.cptr,
            cap_data: PhantomCap::phantom_instance(),
            _role: PhantomData,
        })
    }

    pub(super) fn reservation_iter<Count: Unsigned>(
        self,
    ) -> (
        impl Iterator<Item = LocalCap<MappedPageTable<U1>>>,
        LocalCap<MappedPageTable<Diff<FreeSlots, Count>>>,
    )
    where
        FreeSlots: Sub<Count>,
        Diff<FreeSlots, Count>: Unsigned,
    {
        let iter_cptr = self.cptr;
        let iter_base_vaddr = self.cap_data.vaddr;

        (
            (self.cap_data.next_free_slot..self.cap_data.next_free_slot + Count::to_usize()).map(
                move |slot| {
                    Cap {
                        cptr: iter_cptr,
                        _role: PhantomData,
                        cap_data: MappedPageTable {
                            next_free_slot: slot,
                            vaddr: iter_base_vaddr, //item_vaddr,
                            _free_slots: PhantomData,
                        },
                    }
                },
            ),
            self.skip_pages::<Count>(),
        )
    }

    pub(super) fn skip_pages<Count: Unsigned>(
        self,
    ) -> LocalCap<MappedPageTable<Diff<FreeSlots, Count>>>
    where
        FreeSlots: Sub<Count>,
        Diff<FreeSlots, Count>: Unsigned,
    {
        Cap {
            cptr: self.cptr,
            _role: PhantomData,
            cap_data: MappedPageTable {
                next_free_slot: (self.cap_data.next_free_slot + Count::to_usize()),
                vaddr: self.cap_data.vaddr,
                _free_slots: PhantomData,
            },
        }
    }

    pub(super) fn skip_remaining_pages(self) -> LocalCap<MappedPageTable<U0>>
    where
        FreeSlots: Sub<FreeSlots, Output = U0>,
    {
        self.skip_pages::<FreeSlots>()
    }
}

impl Cap<MappedPage, role::Local> {
    pub fn unmap(self) -> Result<Cap<UnmappedPage, role::Local>, SeL4Error> {
        let err = unsafe { seL4_ARM_Page_Unmap(self.cptr) };
        if err != 0 {
            return Err(SeL4Error::UnmapPage(err));
        }
        Ok(Cap {
            cptr: self.cptr,
            cap_data: PhantomCap::phantom_instance(),
            _role: PhantomData,
        })
    }
}
