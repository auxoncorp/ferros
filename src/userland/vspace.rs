use core::marker::PhantomData;
use core::ops::Sub;
use crate::pow::Pow;
use crate::userland::{
    paging, role, AssignedPageDirectory, Cap, CapRights, LocalCap, MappedPage, MappedPageTable,
    PhantomCap, SeL4Error, UnmappedPage, UnmappedPageTable,
};
use sel4_sys::*;
use typenum::operator_aliases::{Diff, Sub1};
use typenum::{Unsigned, B1, U1};

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
