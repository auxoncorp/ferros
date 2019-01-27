use core::marker::PhantomData;
use core::ops::{Add, Sub};
use crate::pow::Pow;
use crate::userland::cap::ThreadControlBlock;
use crate::userland::process::{setup_initial_stack_and_regs, RetypeForSetup, SetupVer};
use crate::userland::{
    paging, role, ASIDPool, AssignedPageDirectory, BootInfo, Cap, CapRights, ChildCNode,
    FaultSource, LocalCNode, LocalCap, MappedPage, MappedPageTable, PhantomCap, SeL4Error,
    UnassignedPageDirectory, UnmappedPage, UnmappedPageTable, Untyped,
};
use generic_array::sequence::Concat;
use generic_array::{arr, arr_impl, ArrayLength, GenericArray};
use sel4_sys::*;
use typenum::operator_aliases::{Diff, Sub1, Sum};
use typenum::{Unsigned, B1, U0, U1, U10, U128, U14, U16, U2, U256, U3, U4, U7, U9};

#[derive(Debug)]
pub enum VSpaceError {
    ProcessParameterTooBigForStack,
    ProcessParameterHandoffSizeMismatch,
    SeL4Error(SeL4Error),
}

impl From<SeL4Error> for VSpaceError {
    fn from(s: SeL4Error) -> Self {
        VSpaceError::SeL4Error(s)
    }
}

/// A VSpace instance represents the virtual memory space
/// intended to be associated with a particular process,
/// and is used in the setup and creation of that process.
///
/// A VSpace instance comes with with user-image code
/// of the running feL4 application already copied into
/// its internal paging structures.
pub struct VSpace<
    PageDirFreeSlots: Unsigned = U0,
    PageTableFreeSlots: Unsigned = U0,
    FilledPageTableCount: Unsigned = U0,
> where
    FilledPageTableCount: ArrayLength<LocalCap<MappedPageTable<U0>>>,
{
    page_dir: LocalCap<AssignedPageDirectory<PageDirFreeSlots>>,
    current_page_table: LocalCap<MappedPageTable<PageTableFreeSlots>>,
    filled_page_tables: GenericArray<LocalCap<MappedPageTable<U0>>, FilledPageTableCount>,
}

impl VSpace {
    pub fn new<
        ASIDPoolFreeSlots: Unsigned,
        CNodeFreeSlots: Unsigned,
        BootInfoPageDirFreeSlots: Unsigned,
    >(
        boot_info: BootInfo<ASIDPoolFreeSlots, BootInfoPageDirFreeSlots>,
        ut16: LocalCap<Untyped<U16>>,
        dest_cnode: LocalCap<LocalCNode<CNodeFreeSlots>>,
    ) -> Result<
        (
            VSpace<
                Diff<paging::BasePageDirFreeSlots, U2>,
                paging::BasePageTableFreeSlots,
                U1, // FilledPageTableCount
            >,
            BootInfo<Sub1<ASIDPoolFreeSlots>, BootInfoPageDirFreeSlots>,
            // dest_cnode
            LocalCap<LocalCNode<Diff<CNodeFreeSlots, U256>>>,
        ),
        SeL4Error,
    >
    where
        CNodeFreeSlots: Sub<U256>,
        Diff<CNodeFreeSlots, U256>: Unsigned,

        ASIDPoolFreeSlots: Sub<B1>,
        Sub1<ASIDPoolFreeSlots>: Unsigned,
    {
        let (cnode, dest_cnode) = dest_cnode.reserve_region::<U256>();

        let (ut14, page_dir_ut, _, _, cnode) = ut16.quarter(cnode)?;
        let (ut12, _, _, _, cnode) = ut14.quarter(cnode)?;
        let (ut10, initial_page_table_ut, second_page_table_ut, _, cnode) = ut12.quarter(cnode)?;
        let (ut8, _, _, _, cnode) = ut10.quarter(cnode)?;
        let (ut6, _, _, _, cnode) = ut8.quarter(cnode)?;
        let (_, _, _, _, cnode) = ut6.quarter(cnode)?;

        let (vspace, boot_info, cnode) =
            Self::internal_new(boot_info, page_dir_ut, initial_page_table_ut, cnode)?;

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
            let (copied_page_cap, _) = page_cap.copy(&cnode, slot_cnode, CapRights::R)?;
            let _mapped_page = page_slot.map_page(copied_page_cap)?;
        }

        // Let the user start with a fresh page table since we have plenty of
        // unused CNode and Untyped capacity hanging around in here.
        let (vspace, _cnode) = vspace.next_page_table(second_page_table_ut, cnode)?;
        Ok((vspace, boot_info, dest_cnode))
    }
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
    fn internal_new<
        ASIDPoolFreeSlots: Unsigned,
        BootInfoPageDirFreeSlots: Unsigned,
        CNodeFreeSlots: Unsigned,
    >(
        // TODO: model ASIDPool capacity at the type level
        boot_info: BootInfo<ASIDPoolFreeSlots, BootInfoPageDirFreeSlots>,
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
            BootInfo<Sub1<ASIDPoolFreeSlots>, BootInfoPageDirFreeSlots>,
            // dest_cnode
            LocalCap<LocalCNode<Diff<CNodeFreeSlots, U2>>>,
        ),
        SeL4Error,
    >
    where
        CNodeFreeSlots: Sub<U2>,
        Diff<CNodeFreeSlots, U2>: Unsigned,

        ASIDPoolFreeSlots: Sub<B1>,
        Sub1<ASIDPoolFreeSlots>: Unsigned,
    {
        let (cnode, dest_cnode) = dest_cnode.reserve_region::<U2>();

        // allocate the page dir and initial page table
        let (page_dir, cnode): (LocalCap<UnassignedPageDirectory>, _) =
            page_dir_ut.retype_local(cnode)?;

        let (initial_page_table, _cnode): (LocalCap<UnmappedPageTable>, _) =
            page_table_ut.retype_local(cnode)?;

        // assign the page dir and map in the initial page table.
        let (page_dir, boot_info) = boot_info.assign_minimal_page_dir(page_dir)?;
        let (initial_page_table, page_dir) = page_dir.map_page_table(initial_page_table)?;

        Ok((
            VSpace {
                page_dir,
                current_page_table: initial_page_table,
                filled_page_tables: arr![LocalCap<MappedPageTable<U0>>;],
            },
            boot_info,
            dest_cnode,
        ))
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

    pub fn map_page(
        self,
        page: LocalCap<UnmappedPage>,
    ) -> Result<
        (
            LocalCap<MappedPage>,
            VSpace<PageDirFreeSlots, Sub1<PageTableFreeSlots>, FilledPageTableCount>,
        ),
        SeL4Error,
    >
    where
        PageTableFreeSlots: Sub<B1>,
        Sub1<PageTableFreeSlots>: Unsigned,
    {
        let mut page_dir = self.page_dir;
        let (mapped_page, page_table) = self.current_page_table.map_page(page, &mut page_dir)?;
        Ok((
            mapped_page,
            VSpace {
                page_dir: page_dir,
                current_page_table: page_table,
                filled_page_tables: self.filled_page_tables,
            },
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

    pub fn prepare_thread<
        T: RetypeForSetup,
        LocalCNodeFreeSlots: Unsigned,
        ScratchPageTableSlots: Unsigned,
        LocalPageDirFreeSlots: Unsigned,
    >(
        self,
        function_descriptor: extern "C" fn(T) -> (),
        process_parameter: SetupVer<T>,
        untyped: LocalCap<Untyped<U14>>,
        local_cnode: LocalCap<LocalCNode<LocalCNodeFreeSlots>>,
        // TODO: We should index MappedPageTable, MappedPage, and VSpace by role to indicate what address space we're dealing with.
        scratch_page_table: &mut LocalCap<MappedPageTable<ScratchPageTableSlots>>,
        mut local_page_dir: &mut LocalCap<AssignedPageDirectory<LocalPageDirFreeSlots>>,
    ) -> Result<
        (
            ReadyThread,
            VSpace<PageDirFreeSlots, Sub1<Sub1<PageTableFreeSlots>>, FilledPageTableCount>,
            LocalCap<LocalCNode<Diff<LocalCNodeFreeSlots, U9>>>,
        ),
        VSpaceError,
    >
    where
        // TODO - Expect to change this to support subtracting 4 page table slots thanks to guards
        PageTableFreeSlots: Sub<B1>,
        Sub1<PageTableFreeSlots>: Unsigned,

        Sub1<PageTableFreeSlots>: Sub<B1>,
        Sub1<Sub1<PageTableFreeSlots>>: Unsigned,

        LocalCNodeFreeSlots: Sub<U9>,
        Diff<LocalCNodeFreeSlots, U9>: Unsigned,

        ScratchPageTableSlots: Sub<B1>,
        Sub1<ScratchPageTableSlots>: Unsigned,
    {
        // TODO - parameterize this function with Count in order
        // take more than one page for the stack. Requires:
        //   * Use of CNode's reservation_iter
        //   * Getting a handle on the first page (or few pages?)
        // for the params-insertion despite iter-use
        //   * Connecting the Count to the size of the untyped parameter
        //   * Either an iterator over the split-out untypeds
        //   * Or a private/internal bulk retype-local

        // TODO - lift these checks to compile-time, as static assertions
        // Note - This comparison is conservative because technically
        // we can fit some of the params into available registers.
        if core::mem::size_of::<SetupVer<T>>() > paging::PageBytes::USIZE {
            return Err(VSpaceError::ProcessParameterTooBigForStack);
        }
        if core::mem::size_of::<SetupVer<T>>() != core::mem::size_of::<T>() {
            return Err(VSpaceError::ProcessParameterHandoffSizeMismatch);
        }

        // TODO - RESTORE - Reserve a guard page before the stack
        //let mut vspace = self.skip_pages::<U1>();
        let mut vspace = self;
        let (local_cnode, output_cnode) = local_cnode.reserve_region::<U9>();

        let (ut12, stack_page_ut, ipc_buffer_ut, _, local_cnode) = untyped.quarter(local_cnode)?;
        let (ut10, _, _, tcb_ut, local_cnode) = ut12.quarter(local_cnode)?;
        let (stack_page, local_cnode): (Cap<UnmappedPage, _>, _) =
            stack_page_ut.retype_local(local_cnode)?;

        // map the child stack into local memory so we can set it up
        let ((mut registers, param_size_on_stack), stack_page) = scratch_page_table
            .temporarily_map_page(stack_page, &mut local_page_dir, |mapped_page| unsafe {
                setup_initial_stack_and_regs(
                    &process_parameter as *const SetupVer<T> as *const usize,
                    core::mem::size_of::<SetupVer<T>>(),
                    (mapped_page.cap_data.vaddr + (1 << paging::PageBits::USIZE)) as *mut usize,
                )
            })?;
        // Map the stack to the target address space
        let (stack_page, vspace) = vspace.map_page(stack_page)?;
        let stack_pointer =
            stack_page.cap_data.vaddr + (1 << paging::PageBits::USIZE) - param_size_on_stack;

        registers.sp = stack_pointer;
        registers.pc = function_descriptor as seL4_Word;
        registers.r14 = (yield_forever as *const fn() -> ()) as seL4_Word;

        // TODO - RESTORE - Reserve a guard page after the stack
        //let vspace = self.skip_pages::<U1>();

        // Allocate and map the ipc buffer
        let (ipc_buffer, local_cnode) = ipc_buffer_ut.retype_local(local_cnode)?;
        let (ipc_buffer, vspace) = vspace.map_page(ipc_buffer)?;

        // allocate the thread control block
        let (tcb, local_cnode) = tcb_ut.retype_local(local_cnode)?;

        let ready_thread = ReadyThread {
            vspace_cptr: vspace.page_dir.cptr,
            registers,
            ipc_buffer,
            tcb,
        };

        Ok((ready_thread, vspace, output_cnode))
    }
}

pub struct ReadyThread {
    registers: seL4_UserContext,
    vspace_cptr: usize,
    ipc_buffer: LocalCap<MappedPage>,
    tcb: LocalCap<ThreadControlBlock>,
}

impl ReadyThread {
    pub fn start<CSpaceRootFreeSlots: Unsigned>(
        self,
        cspace: LocalCap<ChildCNode<CSpaceRootFreeSlots>>,
        fault_source: Option<FaultSource<role::Child>>,
        // TODO: index tcb by priority, so you can't set a higher priority than
        // the authority (which is a runtime error)
        priority_authority: &LocalCap<ThreadControlBlock>,
        priority: u8,
    ) -> Result<(), SeL4Error> {
        let mut tcb = self.tcb;
        let mut regs = self.registers;

        // configure the tcb
        tcb.configure(cspace, fault_source, self.vspace_cptr, self.ipc_buffer)?;

        unsafe {
            let err = seL4_TCB_WriteRegisters(
                tcb.cptr,
                0,
                0,
                // all the regs
                core::mem::size_of::<seL4_UserContext>() / core::mem::size_of::<seL4_Word>(),
                &mut regs,
            );
            if err != 0 {
                return Err(SeL4Error::TCBWriteRegisters(err));
            }

            let err = seL4_TCB_SetPriority(tcb.cptr, priority_authority.cptr, priority as usize);
            if err != 0 {
                return Err(SeL4Error::TCBSetPriority(err));
            }

            let err = seL4_TCB_Resume(tcb.cptr);
            if err != 0 {
                return Err(SeL4Error::TCBResume(err));
            }
        }

        Ok(())
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

impl<FreeSlots: Unsigned> Cap<ASIDPool<FreeSlots>, role::Local> {
    pub fn assign_minimal(
        self,
        page_dir: LocalCap<UnassignedPageDirectory>,
    ) -> Result<
        (
            LocalCap<AssignedPageDirectory<paging::BasePageDirFreeSlots>>,
            LocalCap<ASIDPool<Sub1<FreeSlots>>>,
        ),
        SeL4Error,
    >
    where
        FreeSlots: Sub<B1>,
        Sub1<FreeSlots>: Unsigned,
    {
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

        Ok((
            page_dir,
            Cap {
                cptr: self.cptr,
                _role: PhantomData,
                cap_data: ASIDPool {
                    next_free_slot: self.cap_data.next_free_slot + 1,
                    _free_slots: PhantomData,
                },
            },
        ))
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
                // TODO:allow vm attrs to be specified in as paramters
                seL4_ARM_VMAttributes_seL4_ARM_PageCacheable
                    | seL4_ARM_VMAttributes_seL4_ARM_ParityEnabled,
                // | seL4_ARM_VMAttributes_seL4_ARM_ExecuteNever
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

        // unsafe {
        //     let addr = seL4_ARM_Page_GetAddress(page.cptr);
        //     debug_println!(
        //         "...mapping page at {:#08x} to {}/{:#08x}",
        //         addr.paddr,
        //         page_dir.cptr,
        //         page_vaddr
        //     );
        // }

        let err = unsafe {
            seL4_ARM_Page_Map(
                page.cptr,
                page_dir.cptr,
                page_vaddr,
                CapRights::RW.into(), // rights
                // TODO:allow vm attrs to be specified in as paramters
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

fn yield_forever() {
    unsafe {
        loop {
            seL4_Yield();
        }
    }
}
