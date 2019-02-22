use core::marker::PhantomData;
use core::ops::{Add, Sub};
use crate::pow::Pow;
use crate::userland::cap::ThreadControlBlock;
use crate::userland::process::{setup_initial_stack_and_regs, RetypeForSetup, SetupVer};
use crate::userland::{
    address_space, memory_kind, paging, role, ASIDPool, AssignedPageDirectory, BootInfo, CNodeRole,
    Cap, CapRange, CapRights, ChildCNode, FaultSource, ImmobileIndelibleInertCapabilityReference,
    LocalCNode, LocalCap, MappedPage, MappedPageTable, MappedSection, MemoryKind, PhantomCap,
    SeL4Error, UnassignedPageDirectory, UnmappedPage, UnmappedPageTable, UnmappedSection, Untyped,
};
use generic_array::{ArrayLength, GenericArray};
use sel4_sys::*;
use typenum::operator_aliases::{Diff, Prod, Sub1, Sum};
use typenum::{
    IsLessOrEqual, UInt, UTerm, Unsigned, B0, B1, U0, U1, U10, U100, U128, U14, U15, U16, U17, U2,
    U32, U5, U6, U8, U9,
};

use core::iter::FromIterator;

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
    Role: CNodeRole = role::Child,
> {
    page_dir: LocalCap<AssignedPageDirectory<PageDirFreeSlots, Role>>,
    current_page_table: LocalCap<MappedPageTable<PageTableFreeSlots, Role>>,
}

type NewVSpaceCNodeSlots = Sum<Sum<paging::CodePageTableCount, paging::CodePageCount>, U16>;
#[rustfmt::skip]
type NewVSpaceCNodeSlotsNormalized = UInt<UInt<UInt<UInt<UInt<UInt<UInt<UInt<UInt<UInt<UInt<UInt<UInt<UInt<UInt<UTerm, B1>, B0>, B0>, B0>, B0>, B0>, B0>, B0>, B1>, B0>, B1>, B0>, B0>, B0>, B0>;

impl VSpace {
    pub fn new<
        ASIDPoolFreeSlots: Unsigned,
        CNodeFreeSlots: Unsigned,
        BootInfoPageDirFreeSlots: Unsigned,
    >(
        boot_info: BootInfo<ASIDPoolFreeSlots, BootInfoPageDirFreeSlots>,
        ut17: LocalCap<Untyped<U17>>,
        dest_cnode: LocalCap<LocalCNode<CNodeFreeSlots>>,
    ) -> Result<
        (
            VSpace<
                Diff<paging::BasePageDirFreeSlots, Sum<paging::CodePageTableCount, U1>>,
                paging::BasePageTableFreeSlots,
                role::Child,
            >,
            BootInfo<Sub1<ASIDPoolFreeSlots>, BootInfoPageDirFreeSlots>,
            // dest_cnode
            LocalCap<LocalCNode<Diff<CNodeFreeSlots, NewVSpaceCNodeSlots>>>,
        ),
        SeL4Error,
    >
    where
        paging::CodePageTableCount: Add<U1>,
        Sum<paging::CodePageTableCount, U1>: Unsigned,

        paging::BasePageTableFreeSlots: Sub<Sum<paging::CodePageTableCount, U1>>,
        Diff<paging::BasePageTableFreeSlots, Sum<paging::CodePageTableCount, U1>>: Unsigned,

        paging::CodePageTableCount: Add<paging::CodePageCount>,
        Sum<paging::CodePageTableCount, paging::CodePageCount>: Unsigned,

        // because of https://github.com/rust-lang/rust/issues/20775, we need to
        // write this trait bound as a fully normalized term
        CNodeFreeSlots: Sub<NewVSpaceCNodeSlotsNormalized>,
        Diff<CNodeFreeSlots, NewVSpaceCNodeSlots>: Unsigned,

        ASIDPoolFreeSlots: Sub<B1>,
        Sub1<ASIDPoolFreeSlots>: Unsigned,
    {
        let (cnode, dest_cnode) = dest_cnode.reserve_region::<NewVSpaceCNodeSlots>();

        let (ut16, page_tables_ut, cnode) = ut17.split(cnode)?;
        let (ut14, page_dir_ut, _, _, cnode) = ut16.quarter(cnode)?;
        let (ut12, _, _, _, cnode) = ut14.quarter(cnode)?;
        let (ut10, initial_page_table_ut, _, _, cnode) = ut12.quarter(cnode)?;

        // allocate and assign the page directory
        let (page_dir, cnode): (LocalCap<UnassignedPageDirectory>, _) =
            page_dir_ut.retype_local(cnode)?;
        let (page_dir, boot_info) = boot_info.assign_minimal_page_dir(page_dir)?;

        // Allocate and map the user image paging structures. This happens
        // first, so it starts at address 0, which is where the code expects to
        // live.

        // Allocate the maximum number of page tables we could possibly need,
        // and reserve that many slots in the page directory.
        let (page_tables, cnode): (
            CapRange<UnmappedPageTable, role::Local, paging::CodePageTableCount>,
            _,
        ) = page_tables_ut.retype_multi(cnode)?;

        let (page_dir_slot_reservation_iter, mut page_dir) =
            page_dir.reservation_iter::<paging::CodePageTableCount>();

        for ((pt, page_dir_slot), _) in page_tables
            .iter()
            .zip(page_dir_slot_reservation_iter)
            // Zipping with an iterator over the page tables from bootinfo limits
            // the iteration to the number of page tables that are actually needed.
            .zip(boot_info.user_image_page_tables_iter())
        {
            let _mapped_pt = page_dir_slot.map_page_table(pt)?;
        }

        // map pages
        let (cnode_slot_reservation_iter, cnode) =
            cnode.reservation_iter::<paging::CodePageCount>();

        for (page_cap, slot_cnode) in boot_info
            .user_image_pages_iter()
            .zip(cnode_slot_reservation_iter)
        {
            // This is RW so that mutable global variables can be used
            let (copied_page_cap, _) = page_cap.copy(&cnode, slot_cnode, CapRights::RW)?;
            // Use map_page_direct instead of a VSpace so we don't have to keep
            // track of bulk allocations which cross page table boundaries at
            // the type level.
            let mapped_page = page_dir.map_page_direct(
                copied_page_cap,
                page_cap.cap_data.vaddr,
                CapRights::RW,
            )?;
        }

        let (initial_page_table, cnode): (LocalCap<UnmappedPageTable>, _) =
            initial_page_table_ut.retype_local(cnode)?;
        let (initial_page_table, page_dir) = page_dir.map_page_table(initial_page_table)?;
        let vspace = VSpace {
            page_dir: page_dir,
            current_page_table: initial_page_table,
        };

        Ok((vspace, boot_info, dest_cnode))
    }
}

impl<PageDirFreeSlots: Unsigned, PageTableFreeSlots: Unsigned, Role: CNodeRole>
    VSpace<PageDirFreeSlots, PageTableFreeSlots, Role>
{
    pub fn next_page_table<CNodeFreeSlots: Unsigned>(
        self,
        new_page_table_ut: LocalCap<Untyped<U10>>,
        dest_cnode: LocalCap<LocalCNode<CNodeFreeSlots>>,
    ) -> Result<
        (
            VSpace<Sub1<PageDirFreeSlots>, paging::BasePageTableFreeSlots, Role>,
            LocalCap<LocalCNode<Sub1<CNodeFreeSlots>>>,
        ),
        SeL4Error,
    >
    where
        PageDirFreeSlots: Sub<B1>,
        Sub1<PageDirFreeSlots>: Unsigned,

        PageTableFreeSlots: Sub<PageTableFreeSlots, Output = U0>,

        CNodeFreeSlots: Sub<B1>,
        Sub1<CNodeFreeSlots>: Unsigned,
    {
        let (new_page_table, dest_cnode): (LocalCap<UnmappedPageTable>, _) =
            new_page_table_ut.retype_local(dest_cnode)?;
        let (new_page_table, page_dir) = self.page_dir.map_page_table(new_page_table)?;
        let _former_current_page_table = self.current_page_table.skip_remaining_pages();

        Ok((
            VSpace {
                page_dir: page_dir,
                current_page_table: new_page_table,
            },
            dest_cnode,
        ))
    }

    pub fn map_page<Kind: MemoryKind>(
        self,
        page: LocalCap<UnmappedPage<Kind>>,
    ) -> Result<
        (
            LocalCap<MappedPage<Role, Kind>>,
            VSpace<PageDirFreeSlots, Sub1<PageTableFreeSlots>, Role>,
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
            },
        ))
    }

    pub fn map_device_page<Kind: MemoryKind>(
        self,
        page: LocalCap<UnmappedPage<Kind>>,
    ) -> Result<
        (
            LocalCap<MappedPage<Role, Kind>>,
            VSpace<PageDirFreeSlots, Sub1<PageTableFreeSlots>, Role>,
        ),
        SeL4Error,
    >
    where
        PageTableFreeSlots: Sub<B1>,
        Sub1<PageTableFreeSlots>: Unsigned,
    {
        let mut page_dir = self.page_dir;
        let (mapped_page, page_table) = self
            .current_page_table
            .map_device_page(page, &mut page_dir)?;
        Ok((
            mapped_page,
            VSpace {
                page_dir: page_dir,
                current_page_table: page_table,
            },
        ))
    }

    pub(super) fn skip_pages<Count: Unsigned>(
        self,
    ) -> VSpace<PageDirFreeSlots, Diff<PageTableFreeSlots, Count>, Role>
    where
        PageTableFreeSlots: Sub<Count>,
        Diff<PageTableFreeSlots, Count>: Unsigned,
    {
        VSpace {
            page_dir: self.page_dir,
            current_page_table: self.current_page_table.skip_pages::<Count>(),
        }
    }

    pub fn skip_remaining_pages(self) -> VSpace<PageDirFreeSlots, U0, Role>
    where
        PageTableFreeSlots: Sub<PageTableFreeSlots, Output = U0>,
    {
        VSpace {
            page_dir: self.page_dir,
            current_page_table: self.current_page_table.skip_remaining_pages(),
        }
    }

    /// Reserve `Count` pages from this VSpace. This can be used to limit the
    /// type-interaction with a VSpace to a single call, significantly
    /// simplifying the type signature of a function which takes a VSpace as a
    /// parameter and then takes pages from it multiple times.
    pub fn reserve_pages<Count: Unsigned>(
        self,
    ) -> (
        VSpace<PageDirFreeSlots, Count, Role>,
        VSpace<PageDirFreeSlots, Diff<PageTableFreeSlots, Count>, Role>,
    )
    where
        PageTableFreeSlots: Sub<Count>,
        Diff<PageTableFreeSlots, Count>: Unsigned,
    {
        (
            VSpace {
                page_dir: Cap {
                    cptr: self.page_dir.cptr,
                    _role: PhantomData,
                    cap_data: AssignedPageDirectory {
                        next_free_slot: self.page_dir.cap_data.next_free_slot,
                        _free_slots: PhantomData,
                        _role: PhantomData,
                    },
                },
                current_page_table: Cap {
                    cptr: self.current_page_table.cptr,
                    _role: PhantomData,
                    cap_data: MappedPageTable {
                        next_free_slot: self.current_page_table.cap_data.next_free_slot,
                        vaddr: self.current_page_table.cap_data.vaddr,
                        _free_slots: PhantomData,
                        _role: PhantomData,
                    },
                },
            },
            VSpace {
                page_dir: self.page_dir,
                current_page_table: Cap {
                    cptr: self.current_page_table.cptr,
                    _role: PhantomData,
                    cap_data: MappedPageTable {
                        next_free_slot: (self.current_page_table.cap_data.next_free_slot
                            + Count::to_usize()),
                        vaddr: self.current_page_table.cap_data.vaddr,
                        _free_slots: PhantomData,
                        _role: PhantomData,
                    },
                },
            },
        )
    }

    pub(super) fn page_slot_reservation_iter<Count: Unsigned>(
        self,
    ) -> (
        impl Iterator<Item = PageSlot<Role>>,
        VSpace<PageDirFreeSlots, Diff<PageTableFreeSlots, Count>, Role>,
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
                    _role: PhantomData,
                },
            },
            page_table: Cap {
                cptr: page_table_cptr,
                _role: PhantomData,
                cap_data: MappedPageTable {
                    next_free_slot: slot_num,
                    vaddr: page_table_vaddr,
                    _free_slots: PhantomData,
                    _role: PhantomData,
                },
            },
        });

        (iter, self.skip_pages::<Count>())
    }

    pub fn map_pages<Kind: MemoryKind, PageCount: Unsigned>(
        self,
        unmapped_pages: CapRange<UnmappedPage<Kind>, role::Local, PageCount>,
    ) -> Result<
        (
            GenericArray<LocalCap<MappedPage<Role, Kind>>, PageCount>,
            VSpace<PageDirFreeSlots, Diff<PageTableFreeSlots, PageCount>, Role>,
        ),
        SeL4Error,
    >
    where
        PageTableFreeSlots: Sub<PageCount>,
        Diff<PageTableFreeSlots, PageCount>: Unsigned,
        PageCount: ArrayLength<LocalCap<MappedPage<Role, Kind>>>,
    {
        let (slot_iter, new_self) = self.page_slot_reservation_iter::<PageCount>();

        let mapped_pages = unmapped_pages
            .iter()
            .zip(slot_iter)
            .map(|(page, slot)| slot.map_page(page))
            // GenericArray::from_iter will panic if the sizes are different,
            // but they are the same.
            .collect::<Result<GenericArray<_, PageCount>, _>>()?;

        Ok((mapped_pages, new_self))
    }
}

// TODO - Consider making this a parameter of prepare_thread.
type StackPageCount = U16;

// TODO these are bigger than they need to be
type PrepareThreadCNodeSlots = U32;
type PrepareThreadPageTableSlots = U32;
type PrepareThreadScratchPages = U16;

impl<PageDirFreeSlots: Unsigned, PageTableFreeSlots: Unsigned, Role: CNodeRole>
    VSpace<PageDirFreeSlots, PageTableFreeSlots, Role>
{
    pub fn prepare_thread<
        T: RetypeForSetup,
        LocalCNodeFreeSlots: Unsigned,
        ScratchPageTableSlots: Unsigned,
        LocalPageDirFreeSlots: Unsigned,
    >(
        self,
        function_descriptor: extern "C" fn(T) -> (),
        process_parameter: SetupVer<T>,
        ut17: LocalCap<Untyped<U17>>,
        local_cnode: LocalCap<LocalCNode<LocalCNodeFreeSlots>>,
        scratch_page_table: &mut LocalCap<MappedPageTable<ScratchPageTableSlots, role::Local>>,
        mut local_page_dir: &mut LocalCap<
            AssignedPageDirectory<LocalPageDirFreeSlots, role::Local>,
        >,
    ) -> Result<
        (
            ReadyThread<Role>,
            VSpace<PageDirFreeSlots, Diff<PageTableFreeSlots, PrepareThreadPageTableSlots>, Role>,
            LocalCap<LocalCNode<Diff<LocalCNodeFreeSlots, PrepareThreadCNodeSlots>>>,
        ),
        VSpaceError,
    >
    where
        PageTableFreeSlots: Sub<PrepareThreadPageTableSlots>,
        Diff<PageTableFreeSlots, PrepareThreadPageTableSlots>: Unsigned,

        // PrepareThreadPageTableSlots: Cmp<PageTableFreeSlots>,
        LocalCNodeFreeSlots: Sub<PrepareThreadCNodeSlots>,
        Diff<LocalCNodeFreeSlots, PrepareThreadCNodeSlots>: Unsigned,

        ScratchPageTableSlots: Sub<PrepareThreadScratchPages>,
        Diff<ScratchPageTableSlots, PrepareThreadScratchPages>: Unsigned,
    {
        // TODO - lift these checks to compile-time, as static assertions
        // Note - This comparison is conservative because technically
        // we can fit some of the params into available registers.
        if core::mem::size_of::<SetupVer<T>>() > (StackPageCount::USIZE * paging::PageBytes::USIZE)
        {
            return Err(VSpaceError::ProcessParameterTooBigForStack);
        }
        if core::mem::size_of::<SetupVer<T>>() != core::mem::size_of::<T>() {
            return Err(VSpaceError::ProcessParameterHandoffSizeMismatch);
        }

        // reserve resources for internal use
        let (vspace, output_vspace) = self.reserve_pages::<PrepareThreadPageTableSlots>();
        let (local_cnode, output_cnode) = local_cnode.reserve_region::<PrepareThreadCNodeSlots>();

        // retypes
        let (ut16, stack_pages_ut, local_cnode) = ut17.split(local_cnode)?;
        let (ut14, _, _, _, local_cnode) = ut16.quarter(local_cnode)?;
        let (ut12, ipc_buffer_ut, _, _, local_cnode) = ut14.quarter(local_cnode)?;
        let (_ut10, tcb_ut, _, _, local_cnode) = ut12.quarter(local_cnode)?;
        let (stack_pages, local_cnode): (
            CapRange<UnmappedPage<memory_kind::General>, role::Local, StackPageCount>,
            _,
        ) = stack_pages_ut.retype_multi(local_cnode)?;

        // Reserve a guard page before the stack
        let vspace = vspace.skip_pages::<U1>();

        // map the child stack into local memory so we can set it up
        let ((mut registers, param_size_on_stack), stack_pages) = scratch_page_table
            .temporarily_map_pages(stack_pages, &mut local_page_dir, |mapped_pages| unsafe {
                setup_initial_stack_and_regs(
                    &process_parameter as *const SetupVer<T> as *const usize,
                    core::mem::size_of::<SetupVer<T>>(),
                    (mapped_pages[StackPageCount::USIZE - 1].cap_data.vaddr
                        + (1 << paging::PageBits::USIZE)) as *mut usize,
                )
            })?;

        // Map the stack to the target address space
        let (mapped_stack_pages, vspace) = vspace.map_pages(stack_pages)?;
        let stack_pointer = mapped_stack_pages[StackPageCount::USIZE - 1].cap_data.vaddr
            + (1 << paging::PageBits::USIZE)
            - param_size_on_stack;

        registers.sp = stack_pointer;
        registers.pc = function_descriptor as seL4_Word;
        // TODO - Probably ought to attempt to suspend the thread instead of endlessly yielding
        registers.r14 = (yield_forever as *const fn() -> !) as seL4_Word;

        // Reserve a guard page after the stack
        let vspace = vspace.skip_pages::<U1>();

        // Allocate and map the ipc buffer
        let (ipc_buffer, local_cnode) = ipc_buffer_ut.retype_local(local_cnode)?;
        let (ipc_buffer, vspace) = vspace.map_page(ipc_buffer)?;

        // allocate the thread control block
        let (tcb, _local_cnode) = tcb_ut.retype_local(local_cnode)?;

        let ready_thread = ReadyThread {
            vspace_cptr: unsafe {
                ImmobileIndelibleInertCapabilityReference::new(vspace.page_dir.cptr)
            },
            registers,
            ipc_buffer,
            tcb,
        };

        Ok((ready_thread, output_vspace, output_cnode))
    }

    pub(crate) fn identity_ref(
        &self,
    ) -> ImmobileIndelibleInertCapabilityReference<AssignedPageDirectory<U0, Role>> {
        unsafe { ImmobileIndelibleInertCapabilityReference::new(self.page_dir.cptr) }
    }
}

impl<PageDirFreeSlots: Unsigned, Role: CNodeRole> VSpace<PageDirFreeSlots, U0, Role> {
    pub fn next_section_vaddr(&self) -> usize {
        self.page_dir.cap_data.next_free_slot
            << (paging::PageBits::USIZE + paging::PageTableBits::USIZE)
    }

    pub fn map_section<Kind: MemoryKind>(
        self,
        section: LocalCap<UnmappedSection<Kind>>,
    ) -> Result<
        (
            LocalCap<MappedSection<Role, Kind>>,
            VSpace<Diff<PageDirFreeSlots, U1>, U0, Role>,
        ),
        SeL4Error,
    >
    where
        PageDirFreeSlots: Sub<U1>,
        Diff<PageDirFreeSlots, U1>: Unsigned,
    {
        let (section, page_dir) = self.page_dir.map_section(section)?;

        Ok((
            section,
            VSpace {
                page_dir: page_dir,
                current_page_table: self.current_page_table,
            },
        ))
    }

    pub(super) fn skip_sections<Count: Unsigned>(
        self,
    ) -> VSpace<Diff<PageDirFreeSlots, Count>, U0, Role>
    where
        PageDirFreeSlots: Sub<Count>,
        Diff<PageDirFreeSlots, Count>: Unsigned,
    {
        VSpace {
            page_dir: self.page_dir.skip_sections::<Count>(),
            current_page_table: self.current_page_table,
        }
    }

    pub fn page_dir_slot_reservation_iter<Count: Unsigned>(
        self,
    ) -> (
        impl Iterator<Item = PageDirSlot<Role>>,
        VSpace<Diff<PageDirFreeSlots, Count>, U0, Role>,
    )
    where
        PageDirFreeSlots: Sub<Count>,
        Diff<PageDirFreeSlots, Count>: Unsigned,
    {
        let start_slot_num = self.page_dir.cap_data.next_free_slot;
        let page_dir_cptr = self.page_dir.cptr;

        let iter =
            (start_slot_num..start_slot_num + Count::USIZE).map(move |slot_num| PageDirSlot {
                page_dir: Cap {
                    cptr: page_dir_cptr,
                    _role: PhantomData,
                    cap_data: AssignedPageDirectory {
                        // this is unused, but we have to fill it out.
                        next_free_slot: slot_num,
                        _free_slots: PhantomData,
                        _role: PhantomData,
                    },
                },
            });

        (iter, self.skip_sections::<Count>())
    }
}

pub struct ReadyThread<Role: CNodeRole> {
    registers: seL4_UserContext,
    vspace_cptr: ImmobileIndelibleInertCapabilityReference<AssignedPageDirectory<U0, Role>>,
    ipc_buffer: LocalCap<MappedPage<Role, memory_kind::General>>,
    tcb: LocalCap<ThreadControlBlock>,
}

impl<Role: CNodeRole> ReadyThread<Role> {
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

/// A slot in a page directory, where a single section or page table goes
pub struct PageDirSlot<Role: CNodeRole> {
    page_dir: LocalCap<AssignedPageDirectory<U1, Role>>,
}

impl<Role: CNodeRole> PageDirSlot<Role> {
    pub fn map_section<Kind: MemoryKind>(
        self,
        section: LocalCap<UnmappedSection<Kind>>,
    ) -> Result<LocalCap<MappedSection<Role, Kind>>, SeL4Error> {
        let (mapped_section, _) = self.page_dir.map_section(section)?;
        Ok(mapped_section)
    }

    pub fn map_page_table(
        self,
        page_table: LocalCap<UnmappedPageTable>,
    ) -> Result<LocalCap<MappedPageTable<paging::BasePageTableFreeSlots, Role>>, SeL4Error> {
        let (mapped_page_table, _) = self.page_dir.map_page_table(page_table)?;
        Ok(mapped_page_table)
    }
}

/// A slot in a page table, where a single page goes
// TODO: This should be named PageTableSlot, for consistency.
pub struct PageSlot<Role: CNodeRole> {
    page_table: LocalCap<MappedPageTable<U1, Role>>,
    page_dir: LocalCap<AssignedPageDirectory<U0, Role>>,
}

impl<Role: CNodeRole> PageSlot<Role> {
    pub fn map_page<Kind: MemoryKind>(
        mut self,
        page: LocalCap<UnmappedPage<Kind>>,
    ) -> Result<LocalCap<MappedPage<Role, Kind>>, SeL4Error> {
        let (res, _) = self.page_table.map_page(page, &mut self.page_dir)?;
        Ok(res)
    }
}

impl<FreeSlots: Unsigned> LocalCap<ASIDPool<FreeSlots>> {
    pub fn assign_minimal(
        self,
        page_dir: LocalCap<UnassignedPageDirectory>,
    ) -> Result<
        (
            LocalCap<AssignedPageDirectory<paging::BasePageDirFreeSlots, role::Child>>,
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
            cap_data: AssignedPageDirectory::<paging::BasePageDirFreeSlots, role::Child> {
                next_free_slot: 0,
                _free_slots: PhantomData,
                _role: PhantomData,
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
impl<FreeSlots: Unsigned, Role: CNodeRole> LocalCap<AssignedPageDirectory<FreeSlots, Role>> {
    pub fn reservation_iter<Count: Unsigned>(
        self,
    ) -> (
        impl Iterator<Item = PageDirSlot<Role>>,
        LocalCap<AssignedPageDirectory<Diff<FreeSlots, Count>, Role>>,
    )
    where
        FreeSlots: Sub<Count>,
        Diff<FreeSlots, Count>: Unsigned,
    {
        let start_slot_num = self.cap_data.next_free_slot;
        let page_dir_cptr = self.cptr;

        let iter =
            (start_slot_num..start_slot_num + Count::USIZE).map(move |slot_num| PageDirSlot {
                page_dir: Cap {
                    cptr: page_dir_cptr,
                    _role: PhantomData,
                    cap_data: AssignedPageDirectory {
                        // this is unused, but we have to fill it out.
                        next_free_slot: slot_num,
                        _free_slots: PhantomData,
                        _role: PhantomData,
                    },
                },
            });

        (iter, self.skip_sections::<Count>())
    }

    pub fn map_page_table(
        self,
        page_table: LocalCap<UnmappedPageTable>,
    ) -> Result<
        (
            LocalCap<MappedPageTable<Pow<paging::PageTableBits>, Role>>,
            LocalCap<AssignedPageDirectory<Sub1<FreeSlots>, Role>>,
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
                    _role: PhantomData,
                },
            },
            // page_dir
            Cap {
                cptr: self.cptr,
                _role: PhantomData,
                cap_data: AssignedPageDirectory {
                    next_free_slot: self.cap_data.next_free_slot + 1,
                    _free_slots: PhantomData,
                    _role: PhantomData,
                },
            },
        ))
    }

    pub fn map_section<Kind: MemoryKind>(
        self,
        section: LocalCap<UnmappedSection<Kind>>,
    ) -> Result<
        (
            LocalCap<MappedSection<Role, Kind>>,
            LocalCap<AssignedPageDirectory<Diff<FreeSlots, U1>, Role>>,
        ),
        SeL4Error,
    >
    where
        FreeSlots: Sub<U1>,
        Diff<FreeSlots, U1>: Unsigned,
    {
        let section_vaddr = self.cap_data.next_free_slot
            << (paging::PageBits::USIZE + paging::PageTableBits::USIZE);

        let err = unsafe {
            seL4_ARM_Page_Map(
                section.cptr,
                self.cptr,
                section_vaddr,
                CapRights::RW.into(), // rights
                seL4_ARM_VMAttributes_seL4_ARM_PageCacheable
                    | seL4_ARM_VMAttributes_seL4_ARM_ParityEnabled
                    // | seL4_ARM_VMAttributes_seL4_ARM_ExecuteNever,
            )
        };
        if err != 0 {
            return Err(SeL4Error::MapPage(err));
        }

        Ok((
            // section
            Cap {
                cptr: section.cptr,
                _role: PhantomData,
                cap_data: MappedSection {
                    vaddr: section_vaddr,
                    _role: PhantomData,
                    _kind: PhantomData,
                },
            },
            // page_dir
            Cap {
                cptr: self.cptr,
                _role: PhantomData,
                cap_data: AssignedPageDirectory {
                    next_free_slot: self.cap_data.next_free_slot + 1,
                    _free_slots: PhantomData,
                    _role: PhantomData,
                },
            },
        ))
    }

    /// Map the given page, assuming that the appropriate paging structures for
    /// the target vaddr have already been set up.
    pub(crate) fn map_page_direct<Kind: MemoryKind>(
        &mut self,
        page: LocalCap<UnmappedPage<Kind>>,
        vaddr: usize,
        rights: CapRights,
    ) -> Result<LocalCap<MappedPage<Role, Kind>>, SeL4Error> {
        let err = unsafe {
            seL4_ARM_Page_Map(
                page.cptr,
                self.cptr,
                vaddr,
                rights.into(),
                seL4_ARM_VMAttributes_seL4_ARM_PageCacheable
                    | seL4_ARM_VMAttributes_seL4_ARM_ParityEnabled
                    // | seL4_ARM_VMAttributes_seL4_ARM_ExecuteNever,
            )
        };
        if err != 0 {
            return Err(SeL4Error::MapPage(err));
        }

        Ok(
            // Page
            Cap {
                cptr: page.cptr,
                _role: PhantomData,
                cap_data: MappedPage {
                    vaddr: vaddr,
                    _role: PhantomData,
                    _kind: PhantomData,
                },
            },
        )
    }

    pub(super) fn skip_sections<Count: Unsigned>(
        self,
    ) -> LocalCap<AssignedPageDirectory<Diff<FreeSlots, Count>, Role>>
    where
        FreeSlots: Sub<Count>,
        Diff<FreeSlots, Count>: Unsigned,
    {
        Cap {
            cptr: self.cptr,
            _role: PhantomData,

            cap_data: AssignedPageDirectory {
                next_free_slot: self.cap_data.next_free_slot + Count::USIZE,
                _free_slots: PhantomData,
                _role: PhantomData,
            },
        }
    }
}

impl<FreeSlots: Unsigned, Role: CNodeRole> LocalCap<MappedPageTable<FreeSlots, Role>> {
    pub fn map_page<PageDirFreeSlots: Unsigned, Kind: MemoryKind>(
        self,
        page: LocalCap<UnmappedPage<Kind>>,
        mut page_dir: &mut LocalCap<AssignedPageDirectory<PageDirFreeSlots, Role>>,
    ) -> Result<
        (
            LocalCap<MappedPage<Role, Kind>>,
            LocalCap<MappedPageTable<Sub1<FreeSlots>, Role>>,
        ),
        SeL4Error,
    >
    where
        FreeSlots: Sub<B1>,
        Sub1<FreeSlots>: Unsigned,
    {
        self.internal_map_page(
            page,
            &mut page_dir,
            CapRights::RW,
            seL4_ARM_VMAttributes_seL4_ARM_PageCacheable
                | seL4_ARM_VMAttributes_seL4_ARM_ParityEnabled,
        )
    }

    pub fn map_device_page<PageDirFreeSlots: Unsigned, Kind: MemoryKind>(
        self,
        page: LocalCap<UnmappedPage<Kind>>,
        mut page_dir: &mut LocalCap<AssignedPageDirectory<PageDirFreeSlots, Role>>,
    ) -> Result<
        (
            LocalCap<MappedPage<Role, Kind>>,
            LocalCap<MappedPageTable<Sub1<FreeSlots>, Role>>,
        ),
        SeL4Error,
    >
    where
        FreeSlots: Sub<B1>,
        Sub1<FreeSlots>: Unsigned,
    {
        self.internal_map_page(page, &mut page_dir, CapRights::RW, 0)
    }

    fn internal_map_page<PageDirFreeSlots: Unsigned, Kind: MemoryKind>(
        self,
        page: LocalCap<UnmappedPage<Kind>>,
        page_dir: &mut LocalCap<AssignedPageDirectory<PageDirFreeSlots, Role>>,
        rights: CapRights,
        attrs: u32,
    ) -> Result<
        (
            LocalCap<MappedPage<Role, Kind>>,
            LocalCap<MappedPageTable<Sub1<FreeSlots>, Role>>,
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
            seL4_ARM_Page_Map(page.cptr, page_dir.cptr, page_vaddr, rights.into(), attrs)
        };
        if err != 0 {
            return Err(SeL4Error::MapPage(err));
        }
        Ok((
            Cap {
                cptr: page.cptr,
                _role: PhantomData,
                cap_data: MappedPage {
                    vaddr: page_vaddr,
                    _role: PhantomData,
                    _kind: PhantomData,
                },
            },
            Cap {
                cptr: self.cptr,
                _role: PhantomData,
                cap_data: MappedPageTable {
                    vaddr: self.cap_data.vaddr,
                    next_free_slot: self.cap_data.next_free_slot + 1,
                    _free_slots: PhantomData,
                    _role: PhantomData,
                },
            },
        ))
    }

    // TODO - Should we restrict this to only be for PageTables in role::Local,
    // since that's mostly the only role that can really meaningfully adjust
    // the content of the page.
    pub fn temporarily_map_page<PageDirFreeSlots: Unsigned, F, Out, Kind: MemoryKind>(
        &mut self,
        unmapped_page: LocalCap<UnmappedPage<Kind>>,
        // TODO - must this page_dir always be the parent of this page table?
        // if so, we should clamp down harder on enforcing this relationship.
        mut page_dir: &mut LocalCap<AssignedPageDirectory<PageDirFreeSlots, Role>>,
        f: F,
    ) -> Result<(Out, LocalCap<UnmappedPage<Kind>>), SeL4Error>
    where
        F: Fn(&LocalCap<MappedPage<Role, Kind>>) -> Out,
        FreeSlots: Sub<B1>,
        Sub1<FreeSlots>: Unsigned,
    {
        // Make a temporary copy of the cap, so we can build on map_page, which
        // requires a move. This is fine because we're unmapping it at the end,
        // ending up with an effectively unmodified page table.
        let temp_page_table: LocalCap<MappedPageTable<FreeSlots, Role>> = Cap {
            cptr: self.cptr,
            _role: PhantomData,
            cap_data: MappedPageTable {
                next_free_slot: self.cap_data.next_free_slot,
                vaddr: self.cap_data.vaddr,
                _free_slots: PhantomData,
                _role: PhantomData,
            },
        };

        let (mapped_page, _) = temp_page_table.map_page(unmapped_page, &mut page_dir)?;
        let res = f(&mapped_page);
        let unmapped_page = mapped_page.unmap()?;

        Ok((res, unmapped_page))
    }

    // TODO - Should we restrict this to only be for PageTables in role::Local,
    // since that's mostly the only role that can really meaningfully adjust
    // the content of the page.
    pub fn temporarily_map_pages<
        PageDirFreeSlots: Unsigned,
        F,
        Out,
        Kind: MemoryKind,
        PageCount: Unsigned,
    >(
        &mut self,
        unmapped_pages: CapRange<UnmappedPage<Kind>, role::Local, PageCount>,
        // TODO - must this page_dir always be the parent of this page table?
        // if so, we should clamp down harder on enforcing this relationship.
        mut page_dir: &mut LocalCap<AssignedPageDirectory<PageDirFreeSlots, Role>>,
        f: F,
    ) -> Result<(Out, CapRange<UnmappedPage<Kind>, role::Local, PageCount>), SeL4Error>
    where
        F: Fn(&GenericArray<LocalCap<MappedPage<Role, Kind>>, PageCount>) -> Out,
        FreeSlots: Sub<PageCount>,
        Diff<FreeSlots, PageCount>: Unsigned,
        // PageCount: ArrayLength<LocalCap<UnmappedPage<Kind>>>,
        PageCount: ArrayLength<LocalCap<MappedPage<Role, Kind>>>,
    {
        // Keep a copy of the caprange to return after it's been consumed by mapping
        let unmapped_pages_copy = CapRange {
            start_cptr: unmapped_pages.start_cptr,
            _cap_type: PhantomData,
            _role: PhantomData,
            _slots: PhantomData,
        };

        // Use page_dir.map_page_direct to avoid altering the type of page_dir.
        // We know from the type constraints that the pages will fit

        let vaddr_iter = (self.cap_data.vaddr..core::usize::MAX).step_by(paging::PageBytes::USIZE);

        // this will panic if the sizes are different, but they are the same.
        let mapped_pages = unmapped_pages
            .iter()
            .zip(vaddr_iter)
            .map(|(page, vaddr)| page_dir.map_page_direct(page, vaddr, CapRights::RW))
            .collect::<Result<GenericArray<_, PageCount>, _>>()?;

        // let (mapped_pages, _) = temp_page_table.map_pages(unmapped_pages, &mut page_dir)?;
        let res = f(&mapped_pages);

        for p in mapped_pages {
            p.unmap()?;
        }

        Ok((res, unmapped_pages_copy))
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
        impl ExactSizeIterator<Item = LocalCap<MappedPageTable<U1, Role>>>,
        LocalCap<MappedPageTable<Diff<FreeSlots, Count>, Role>>,
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
                            _role: PhantomData,
                        },
                    }
                },
            ),
            self.skip_pages::<Count>(),
        )
    }

    pub(super) fn skip_pages<Count: Unsigned>(
        self,
    ) -> LocalCap<MappedPageTable<Diff<FreeSlots, Count>, Role>>
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
                _role: PhantomData,
            },
        }
    }

    pub fn skip_remaining_pages(self) -> LocalCap<MappedPageTable<U0, Role>>
    where
        FreeSlots: Sub<FreeSlots, Output = U0>,
    {
        self.skip_pages::<FreeSlots>()
    }
}

impl<Role: CNodeRole, Kind: MemoryKind> LocalCap<MappedPage<Role, Kind>> {
    pub fn physical_address(self) -> Result<usize, SeL4Error> {
        let getaddr_t = unsafe { seL4_ARM_Page_GetAddress(self.cptr) };
        if getaddr_t.error != 0 {
            return Err(SeL4Error::GetPageAddr(getaddr_t.error as u32));
        }

        Ok(getaddr_t.paddr)
    }

    pub fn unmap(self) -> Result<Cap<UnmappedPage<Kind>, role::Local>, SeL4Error> {
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

impl<Role: CNodeRole, Kind: MemoryKind> LocalCap<MappedSection<Role, Kind>> {
    pub fn physical_address(self) -> Result<usize, SeL4Error> {
        let getaddr_t = unsafe { seL4_ARM_Page_GetAddress(self.cptr) };
        if getaddr_t.error != 0 {
            return Err(SeL4Error::GetPageAddr(getaddr_t.error as u32));
        }

        Ok(getaddr_t.paddr)
    }
}

pub fn yield_forever() -> ! {
    unsafe {
        loop {
            seL4_Yield();
        }
    }
}
