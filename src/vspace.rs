use core::marker::PhantomData;
use core::mem;
use core::ops::{Add, Sub};

use generic_array::{ArrayLength, GenericArray};

use selfe_sys::*;

use typenum::operator_aliases::{Diff, Sub1, Sum};
use typenum::*;

use crate::arch;
use crate::arch::cap::{
    AssignedPageDirectory, MappedPage, MappedPageTable, MappedSection, UnassignedASID,
    UnassignedPageDirectory, UnmappedPage, UnmappedPageTable, UnmappedSection,
};
use crate::bootstrap::UserImage;
use crate::cap::{
    self, memory_kind, role, CNodeRole, Cap, CapRange, ChildCNode, ChildCNodeSlots, DirectRetype,
    ImmobileIndelibleInertCapabilityReference, LocalCNode, LocalCNodeSlot, LocalCNodeSlots,
    LocalCap, MemoryKind, PhantomCap, ThreadControlBlock, ThreadPriorityAuthority, Untyped,
};
use crate::error::SeL4Error;
use crate::pow::Pow;
use crate::userland::process::{setup_initial_stack_and_regs, RetypeForSetup, SetupVer};
use crate::userland::{CapRights, FaultSource};

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
/// of the running sel4 application already copied into
/// its internal paging structures.
pub struct VSpace<
    PageDirFreeSlots: Unsigned = U0,
    PageTableFreeSlots: Unsigned = U0,
    Role: CNodeRole = role::Child,
> {
    page_dir: LocalCap<AssignedPageDirectory<PageDirFreeSlots, Role>>,
    current_page_table: LocalCap<MappedPageTable<PageTableFreeSlots, Role>>,
}

pub type NewVSpaceCNodeSlots = Sum<Sum<arch::CodePageTableCount, arch::CodePageCount>, U16>;

impl VSpace {
    pub fn new(
        ut17: LocalCap<Untyped<U17>>,
        dest_slots: LocalCNodeSlots<NewVSpaceCNodeSlots>,
        asid: LocalCap<UnassignedASID>,
        user_image: &UserImage<role::Local>,
        parent_cnode: &LocalCap<LocalCNode>,
    ) -> Result<
        VSpace<
            Diff<arch::BasePageDirFreeSlots, Sum<arch::CodePageTableCount, U1>>,
            arch::BasePageTableFreeSlots,
            role::Child,
        >,
        SeL4Error,
    >
    where
        arch::CodePageTableCount: Add<U1>,
        Sum<arch::CodePageTableCount, U1>: Unsigned,

        arch::BasePageTableFreeSlots: Sub<Sum<arch::CodePageTableCount, U1>>,
        Diff<arch::BasePageTableFreeSlots, Sum<arch::CodePageTableCount, U1>>: Unsigned,

        arch::CodePageTableCount: Add<arch::CodePageCount>,
        Sum<arch::CodePageTableCount, arch::CodePageCount>: Unsigned,
    {
        VSpace::new_internal(ut17, dest_slots, asid, &user_image, &parent_cnode, None)
    }

    pub fn new_with_writable_user_image(
        ut17: LocalCap<Untyped<U17>>,
        dest_slots: LocalCNodeSlots<NewVSpaceCNodeSlots>,
        asid: LocalCap<UnassignedASID>,
        user_image: &UserImage<role::Local>,
        parent_cnode: &LocalCap<LocalCNode>,
        code_copy_support_items: (
            &mut VSpaceScratchSlice<role::Local>,
            LocalCap<Untyped<arch::TotalCodeSizeBits>>,
        ),
    ) -> Result<
        VSpace<
            Diff<arch::BasePageDirFreeSlots, Sum<arch::CodePageTableCount, U1>>,
            arch::BasePageTableFreeSlots,
            role::Child,
        >,
        SeL4Error,
    >
    where
        arch::CodePageTableCount: Add<U1>,
        Sum<arch::CodePageTableCount, U1>: Unsigned,

        arch::BasePageTableFreeSlots: Sub<Sum<arch::CodePageTableCount, U1>>,
        Diff<arch::BasePageTableFreeSlots, Sum<arch::CodePageTableCount, U1>>: Unsigned,

        arch::CodePageTableCount: Add<arch::CodePageCount>,
        Sum<arch::CodePageTableCount, arch::CodePageCount>: Unsigned,
    {
        VSpace::new_internal(
            ut17,
            dest_slots,
            asid,
            &user_image,
            &parent_cnode,
            Some(code_copy_support_items),
        )
    }

    fn new_internal(
        ut17: LocalCap<Untyped<U17>>,
        dest_slots: LocalCNodeSlots<NewVSpaceCNodeSlots>,
        asid: LocalCap<UnassignedASID>,
        user_image: &UserImage<role::Local>,
        parent_cnode: &LocalCap<LocalCNode>,
        code_copy_support_items: Option<(
            &mut VSpaceScratchSlice<role::Local>,
            LocalCap<Untyped<arch::TotalCodeSizeBits>>,
        )>,
    ) -> Result<
        VSpace<
            Diff<arch::BasePageDirFreeSlots, Sum<arch::CodePageTableCount, U1>>,
            arch::BasePageTableFreeSlots,
            role::Child,
        >,
        SeL4Error,
    >
    where
        arch::CodePageTableCount: Add<U1>,
        Sum<arch::CodePageTableCount, U1>: Unsigned,

        arch::BasePageTableFreeSlots: Sub<Sum<arch::CodePageTableCount, U1>>,
        Diff<arch::BasePageTableFreeSlots, Sum<arch::CodePageTableCount, U1>>: Unsigned,

        arch::CodePageTableCount: Add<arch::CodePageCount>,
        Sum<arch::CodePageTableCount, arch::CodePageCount>: Unsigned,

        ScratchPageTableSlots: Sub<B1>,
        Sub1<ScratchPageTableSlots>: Unsigned,
        ScratchPageTableSlots: IsGreaterOrEqual<U1, Output = True>,
    {
        let (slots, dest_slots) = dest_slots.alloc();
        let (ut16, page_tables_ut) = ut17.split(slots)?;

        let (slots, dest_slots) = dest_slots.alloc();
        let (ut14, page_dir_ut, _, _) = ut16.quarter(slots)?;

        let (slots, dest_slots) = dest_slots.alloc();
        let (ut12, _, _, _) = ut14.quarter(slots)?;

        let (slots, dest_slots) = dest_slots.alloc();
        let (_ut10, initial_page_table_ut, _, _) = ut12.quarter(slots)?;

        // allocate and assign the page directory
        let (slots, dest_slots) = dest_slots.alloc();
        let page_dir: LocalCap<UnassignedPageDirectory> = page_dir_ut.retype(slots)?;
        let (_asid, page_dir) = asid.assign(page_dir)?;

        // Allocate and map the user image paging structures. This happens
        // first, so it starts at address 0, which is where the code expects to
        // live.

        // Allocate the maximum number of page tables we could possibly need,
        // and reserve that many slots in the page directory.
        let (slots, dest_slots) = dest_slots.alloc();
        let page_tables: CapRange<UnmappedPageTable, role::Local, arch::CodePageTableCount> =
            page_tables_ut.retype_multi(slots)?;

        let (page_dir_slot_reservation_iter, mut page_dir) =
            page_dir.reservation_iter::<arch::CodePageTableCount>();

        for (pt, page_dir_slot) in page_tables
            .iter()
            .zip(page_dir_slot_reservation_iter)
            .take(user_image.page_table_count())
        {
            let _mapped_pt = page_dir_slot.map_page_table(pt)?;
        }

        // Here we determine whether or not this burgeoning process
        // needs writable access to the user image (e.g., for global
        // variables). We're signalled to this by the presence of a
        // scratch page table and a 26-bit untyped in which we can
        // retype into pages to hold the user image.
        let (slots, dest_slots) = dest_slots.alloc::<arch::CodePageCount>();
        match code_copy_support_items {
            Some((scratch, code_ut)) => {
                // First, retype the untyped into `CodePageCount`
                // pages.
                let fresh_pages: CapRange<
                    UnmappedPage<memory_kind::General>,
                    role::Local,
                    arch::CodePageCount,
                > = code_ut.retype_multi(slots)?;
                // Then, zip up the pages with the user image pages
                for (ui_page, fresh_page) in user_image.pages_iter().zip(fresh_pages.iter()) {
                    // Temporarily map the new page and copy the data
                    // from `user_image` to the new page.
                    let (_, fresh_unmapped_page) =
                        scratch.temporarily_map_page(fresh_page, |temp_mapped_page| {
                            unsafe {
                                *(mem::transmute::<usize, *mut [usize; arch::WORDS_PER_PAGE]>(
                                    temp_mapped_page.cap_data.vaddr,
                                )) = *(mem::transmute::<
                                    usize,
                                    *const [usize; arch::WORDS_PER_PAGE],
                                >(ui_page.cap_data.vaddr))
                            };
                        })?;
                    // Finally, map that page into the target vspace
                    // /at the same virtual address/. This is where
                    // the code is expected to be.
                    let _ = page_dir.map_page_direct(
                        fresh_unmapped_page,
                        ui_page.cap_data.vaddr,
                        CapRights::RW,
                    )?;
                }
            }
            None => {
                // map pages
                for (page_cap, slot) in user_image.pages_iter().zip(slots.iter()) {
                    let copied_page_cap = page_cap.copy(&parent_cnode, slot, CapRights::R)?;
                    // Use map_page_direct instead of a VSpace so we don't have to keep
                    // track of bulk allocations which cross page table boundaries at
                    // the type level.
                    let _ = page_dir.map_page_direct(
                        copied_page_cap,
                        page_cap.cap_data.vaddr,
                        CapRights::R,
                    )?;
                }
            }
        };

        let (slots, _dest_slots) = dest_slots.alloc();
        let initial_page_table: LocalCap<UnmappedPageTable> =
            initial_page_table_ut.retype(slots)?;
        let (initial_page_table, page_dir) = page_dir.map_page_table(initial_page_table)?;

        let vspace = VSpace {
            page_dir: page_dir,
            current_page_table: initial_page_table,
        };

        Ok(vspace)
    }
}

impl<PageDirFreeSlots: Unsigned, PageTableFreeSlots: Unsigned, Role: CNodeRole>
    VSpace<PageDirFreeSlots, PageTableFreeSlots, Role>
{
    pub fn next_page_table(
        self,
        new_page_table_ut: LocalCap<Untyped<<UnmappedPageTable as DirectRetype>::SizeBits>>,
        dest_slot: LocalCNodeSlot,
    ) -> Result<VSpace<Sub1<PageDirFreeSlots>, arch::BasePageTableFreeSlots, Role>, SeL4Error>
    where
        PageDirFreeSlots: Sub<B1>,
        Sub1<PageDirFreeSlots>: Unsigned,

        PageTableFreeSlots: Sub<PageTableFreeSlots, Output = U0>,
    {
        let new_page_table: LocalCap<UnmappedPageTable> = new_page_table_ut.retype(dest_slot)?;
        let (new_page_table, page_dir) = self.page_dir.map_page_table(new_page_table)?;
        let _former_current_page_table = self.current_page_table.skip_remaining_pages();

        Ok(VSpace {
            page_dir: page_dir,
            current_page_table: new_page_table,
        })
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
pub type PrepareThreadCNodeSlots = U32;
pub type PrepareThreadPageTableSlots = U32;
pub type PrepareThreadScratchPages = U16;

impl<PageDirFreeSlots: Unsigned, PageTableFreeSlots: Unsigned, Role: CNodeRole>
    VSpace<PageDirFreeSlots, PageTableFreeSlots, Role>
{
    pub fn prepare_thread<T: RetypeForSetup>(
        self,
        function_descriptor: extern "C" fn(T) -> (),
        process_parameter: SetupVer<T>,
        ut17: LocalCap<Untyped<U17>>,
        dest_slots: LocalCNodeSlots<PrepareThreadCNodeSlots>,
        scratch_space: &mut VSpaceScratchSlice<role::Local>,
    ) -> Result<
        (
            ReadyThread<Role>,
            VSpace<PageDirFreeSlots, Diff<PageTableFreeSlots, PrepareThreadPageTableSlots>, Role>,
        ),
        VSpaceError,
    >
    where
        PageTableFreeSlots: Sub<PrepareThreadPageTableSlots>,
        Diff<PageTableFreeSlots, PrepareThreadPageTableSlots>: Unsigned,
    {
        // TODO - lift these checks to compile-time, as static assertions
        // Note - This comparison is conservative because technically
        // we can fit some of the params into available registers.
        if core::mem::size_of::<SetupVer<T>>() > (StackPageCount::USIZE * arch::PageBytes::USIZE) {
            return Err(VSpaceError::ProcessParameterTooBigForStack);
        }
        if core::mem::size_of::<SetupVer<T>>() != core::mem::size_of::<T>() {
            return Err(VSpaceError::ProcessParameterHandoffSizeMismatch);
        }

        // reserve resources for internal use
        let (vspace, output_vspace) = self.reserve_pages::<PrepareThreadPageTableSlots>();

        // retypes
        let (slots, dest_slots) = dest_slots.alloc();
        let (ut16, stack_pages_ut) = ut17.split(slots)?;

        let (slots, dest_slots) = dest_slots.alloc();
        let (ut14, _, _, _) = ut16.quarter(slots)?;

        let (slots, dest_slots) = dest_slots.alloc();
        let (ut12, ipc_buffer_ut, _, _) = ut14.quarter(slots)?;

        let (slots, dest_slots) = dest_slots.alloc();
        let (_ut10, tcb_ut, _, _) = ut12.quarter(slots)?;

        let (slots, dest_slots) = dest_slots.alloc();
        let stack_pages: CapRange<UnmappedPage<memory_kind::General>, role::Local, StackPageCount> =
            stack_pages_ut.retype_multi(slots)?;

        // Reserve a guard page before the stack
        let vspace = vspace.skip_pages::<U1>();

        // map the child stack into local memory so we can set it up
        let ((mut registers, param_size_on_stack), stack_pages) = scratch_space
            .temporarily_map_pages(stack_pages, |mapped_pages| unsafe {
                setup_initial_stack_and_regs(
                    &process_parameter as *const SetupVer<T> as *const usize,
                    core::mem::size_of::<SetupVer<T>>(),
                    (mapped_pages[StackPageCount::USIZE - 1].cap_data.vaddr
                        + (1 << arch::PageBits::USIZE)) as *mut usize,
                )
            })?;

        // Map the stack to the target address space
        let (mapped_stack_pages, vspace) = vspace.map_pages(stack_pages)?;
        let stack_pointer = mapped_stack_pages[StackPageCount::USIZE - 1].cap_data.vaddr
            + (1 << arch::PageBits::USIZE)
            - param_size_on_stack;

        registers.sp = stack_pointer;
        registers.pc = function_descriptor as usize;
        // TODO - Probably ought to attempt to suspend the thread instead of endlessly yielding
        registers.r14 = (yield_forever as *const fn() -> !) as usize;

        // Reserve a guard page after the stack
        let vspace = vspace.skip_pages::<U1>();

        // Allocate and map the ipc buffer
        let (slots, dest_slots) = dest_slots.alloc();
        let ipc_buffer = ipc_buffer_ut.retype(slots)?;
        let (ipc_buffer, vspace) = vspace.map_page(ipc_buffer)?;

        // allocate the thread control block
        let (slots, _dest_slots) = dest_slots.alloc();
        let tcb = tcb_ut.retype(slots)?;

        let ready_thread = ReadyThread {
            vspace_cptr: unsafe {
                ImmobileIndelibleInertCapabilityReference::new(vspace.page_dir.cptr)
            },
            registers,
            ipc_buffer,
            tcb,
        };

        Ok((ready_thread, output_vspace))
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
            << (arch::PageBits::USIZE + arch::PageTableBits::USIZE)
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

    /// Map the given page tables. Reserve all the pages in them. Return an slot
    /// iterator over their page slots.
    pub fn page_block_reservation_iter<PageTableCount: Unsigned>(
        self,
        page_tables: CapRange<UnmappedPageTable, role::Local, PageTableCount>,
    ) -> Result<
        (
            impl Iterator<Item = PageSlot<Role>>,
            VSpace<Diff<PageDirFreeSlots, PageTableCount>, U0, Role>,
        ),
        SeL4Error,
    >
    where
        PageDirFreeSlots: Sub<PageTableCount>,
        Diff<PageDirFreeSlots, PageTableCount>: Unsigned,
        PageTableCount: ArrayLength<LocalCap<MappedPageTable<arch::BasePageTableFreeSlots, Role>>>,
    {
        // map all the page tables
        let (page_dir_slot_iter, vspace) = self.page_dir_slot_reservation_iter::<PageTableCount>();
        let mapped_page_tables = page_tables
            .iter()
            .zip(page_dir_slot_iter)
            .map(|(pt, pd_slot)| pd_slot.map_page_table(pt))
            .collect::<Result<GenericArray<_, PageTableCount>, _>>()?;

        let pd_cptr = vspace.page_dir.cptr;
        let page_slot_iter = mapped_page_tables.into_iter().flat_map(move |pt| {
            // debug_println!("Mapped page table to {:#08x}", pt.cap_data.vaddr);

            let pd_cptr = pd_cptr;
            let pt_cptr = pt.cptr;
            let pt_vaddr = pt.cap_data.vaddr;
            (0..256).map(move |slot_num| PageSlot {
                page_dir: Cap {
                    cptr: pd_cptr,
                    _role: PhantomData,
                    cap_data: AssignedPageDirectory {
                        // this is unused, but we have to fill it out.
                        next_free_slot: core::usize::MAX,
                        _free_slots: PhantomData,
                        _role: PhantomData,
                    },
                },
                page_table: Cap {
                    cptr: pt_cptr,
                    _role: PhantomData,
                    cap_data: MappedPageTable {
                        next_free_slot: slot_num,
                        vaddr: pt_vaddr,
                        _free_slots: PhantomData,
                        _role: PhantomData,
                    },
                },
            })
        });

        Ok((page_slot_iter, vspace))
    }
}
impl<PageDirFreeSlots: Unsigned, PageTableFreeSlots: Unsigned>
    VSpace<PageDirFreeSlots, PageTableFreeSlots, role::Child>
{
    pub fn create_child_scratch(
        self,
        page_tables_ut: LocalCap<Untyped<Sum<<UnmappedPageTable as DirectRetype>::SizeBits, U1>>>,
        local_slots: LocalCNodeSlots<U4>,
        child_slots: ChildCNodeSlots<U2>,
        parent_cnode: &LocalCap<LocalCNode>,
    ) -> Result<
        (
            VSpaceScratchSlice<role::Child>,
            VSpace<Sub1<Sub1<PageDirFreeSlots>>, arch::BasePageTableFreeSlots, role::Child>,
        ),
        SeL4Error,
    >
    where
        PageDirFreeSlots: Sub<B1>,
        Sub1<PageDirFreeSlots>: Unsigned,
        Sub1<PageDirFreeSlots>: Sub<B1>,
        Sub1<Sub1<PageDirFreeSlots>>: Unsigned,

        PageTableFreeSlots: Sub<PageTableFreeSlots, Output = U0>,
    {
        let (page_table_ut_slots, local_slots) = local_slots.alloc();
        let (pt_ut_a, pt_ut_b) = page_tables_ut.split(page_table_ut_slots)?;
        let (page_table_slot, local_slots) = local_slots.alloc();
        // Skip ahead to a fresh page table
        let VSpace {
            page_dir,
            current_page_table,
        } = self.next_page_table(pt_ut_a, page_table_slot)?;

        // Move the (now-fresh) current page table to the child CSpace
        let (child_page_table_slot, child_slots) = child_slots.alloc();
        let child_page_table =
            current_page_table.move_to_slot(parent_cnode, child_page_table_slot)?;

        // Make an effectively immutable copy of the page directory capability
        // in the child CSpace.
        //
        // NB: we don't really want to make AssignedPageDirectory copy-alias-able in public
        // so we use an internal copy method to do the copy-work and get the destination offset.
        // Then we manually create a page directory alias instance that lacks
        // any visible capacity for mutability.
        let child_page_dir = Cap {
            cptr: page_dir.unchecked_copy(parent_cnode, child_slots, CapRights::RWG)?,
            _role: PhantomData,
            cap_data: AssignedPageDirectory {
                next_free_slot: core::usize::MAX,
                _free_slots: PhantomData,
                _role: PhantomData,
            },
        };

        let (new_page_table, page_dir) = page_dir.map_page_table(pt_ut_b.retype(local_slots)?)?;

        Ok((
            VSpaceScratchSlice {
                page_dir: child_page_dir,
                page_table: child_page_table,
            },
            VSpace {
                page_dir,
                current_page_table: new_page_table,
            },
        ))
    }
}

pub struct ReadyThread<Role: CNodeRole> {
    registers: seL4_UserContext,
    vspace_cptr: ImmobileIndelibleInertCapabilityReference<AssignedPageDirectory<U0, Role>>,
    ipc_buffer: LocalCap<MappedPage<Role, memory_kind::General>>,
    tcb: LocalCap<ThreadControlBlock>,
}

impl<Role: CNodeRole> ReadyThread<Role> {
    pub fn start(
        self,
        cspace: LocalCap<ChildCNode>,
        fault_source: Option<FaultSource<role::Child>>,
        // TODO: index tcb by priority, so you can't set a higher priority than
        priority_authority: &LocalCap<ThreadPriorityAuthority>,
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
                core::mem::size_of::<seL4_UserContext>() / core::mem::size_of::<usize>(),
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
    ) -> Result<LocalCap<MappedPageTable<arch::BasePageTableFreeSlots, Role>>, SeL4Error> {
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

    pub fn map_device_page<Kind: MemoryKind>(
        mut self,
        page: LocalCap<UnmappedPage<Kind>>,
    ) -> Result<LocalCap<MappedPage<Role, Kind>>, SeL4Error> {
        let (res, _) = self.page_table.map_device_page(page, &mut self.page_dir)?;
        Ok(res)
    }

    pub fn map_dma_page<Kind: MemoryKind>(
        mut self,
        page: LocalCap<UnmappedPage<Kind>>,
    ) -> Result<LocalCap<MappedPage<Role, Kind>>, SeL4Error> {
        let (res, _) = self.page_table.map_dma_page(page, &mut self.page_dir)?;
        Ok(res)
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
            LocalCap<MappedPageTable<Pow<arch::PageTableBits>, Role>>,
            LocalCap<AssignedPageDirectory<Sub1<FreeSlots>, Role>>,
        ),
        SeL4Error,
    >
    where
        FreeSlots: Sub<B1>,
        Sub1<FreeSlots>: Unsigned,
    {
        let page_table_vaddr =
            self.cap_data.next_free_slot << (arch::PageBits::USIZE + arch::PageTableBits::USIZE);

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
        let section_vaddr =
            self.cap_data.next_free_slot << (arch::PageBits::USIZE + arch::PageTableBits::USIZE);

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

    pub fn map_dma_page<PageDirFreeSlots: Unsigned, Kind: MemoryKind>(
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
        let (res, pt) = self.internal_map_page(page, &mut page_dir, CapRights::RW, 0)?;

        let err = unsafe { seL4_ARM_Page_CleanInvalidate_Data(res.cptr, 0, 0x1000) };
        if err != 0 {
            return Err(SeL4Error::PageCleanInvalidateData(err));
        }

        return Ok((res, pt));
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
        let mapped_page = self.unchecked_map_page(&page, page_dir, rights, attrs)?;
        Ok((
            mapped_page,
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

    fn unchecked_map_page<PageDirFreeSlots: Unsigned, Kind: MemoryKind>(
        &self,
        page: &LocalCap<UnmappedPage<Kind>>,
        page_dir: &mut LocalCap<AssignedPageDirectory<PageDirFreeSlots, Role>>,
        rights: CapRights,
        attrs: u32,
    ) -> Result<LocalCap<MappedPage<Role, Kind>>, SeL4Error> {
        let page_vaddr =
            self.cap_data.vaddr + (self.cap_data.next_free_slot << arch::PageBits::USIZE);

        let err = unsafe {
            seL4_ARM_Page_Map(page.cptr, page_dir.cptr, page_vaddr, rights.into(), attrs)
        };
        if err != 0 {
            return Err(SeL4Error::MapPage(err));
        }
        // debug_println!("Mapped page to {:#10x} in page_dir {}", page_vaddr, page_dir.cptr);
        return Ok(Cap {
            cptr: page.cptr,
            _role: PhantomData,
            cap_data: MappedPage {
                vaddr: page_vaddr,
                _role: PhantomData,
                _kind: PhantomData,
            },
        });
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
        FreeSlots: IsGreaterOrEqual<U1, Output = True>,
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

        let mapped_page = temp_page_table.unchecked_map_page(
            &unmapped_page,
            &mut page_dir,
            CapRights::RW,
            seL4_ARM_VMAttributes_seL4_ARM_PageCacheable
                | seL4_ARM_VMAttributes_seL4_ARM_ParityEnabled,
        )?;
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
        page_dir: &mut LocalCap<AssignedPageDirectory<PageDirFreeSlots, Role>>,
        f: F,
    ) -> Result<(Out, CapRange<UnmappedPage<Kind>, role::Local, PageCount>), SeL4Error>
    where
        F: Fn(&GenericArray<LocalCap<MappedPage<Role, Kind>>, PageCount>) -> Out,
        FreeSlots: IsGreaterOrEqual<PageCount, Output = True>,
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

        let vaddr_iter = (self.cap_data.vaddr..core::usize::MAX).step_by(arch::PageBytes::USIZE);

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
    pub fn virtual_address(&self) -> usize {
        self.cap_data.vaddr
    }

    pub fn physical_address(&self) -> Result<usize, SeL4Error> {
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
    pub fn virtual_address(&self) -> usize {
        self.cap_data.vaddr
    }

    pub fn physical_address(&self) -> Result<usize, SeL4Error> {
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

type ScratchPageTableSlots = Pow<arch::PageTableBits>;
#[derive(Debug)]
pub struct VSpaceScratchSlice<Role: CNodeRole> {
    page_dir: Cap<AssignedPageDirectory<U0, Role>, Role>,
    page_table: Cap<MappedPageTable<ScratchPageTableSlots, Role>, Role>,
}

impl VSpaceScratchSlice<role::Local> {
    pub fn from_parts<FreePageDirSlots: Unsigned>(
        slot: LocalCNodeSlot,
        ut: LocalCap<Untyped<<UnmappedPageTable as DirectRetype>::SizeBits>>,
        page_directory: LocalCap<AssignedPageDirectory<FreePageDirSlots, role::Local>>,
    ) -> Result<
        (
            Self,
            LocalCap<AssignedPageDirectory<Sub1<FreePageDirSlots>, role::Local>>,
        ),
        SeL4Error,
    >
    where
        FreePageDirSlots: Sub<B1>,
        Sub1<FreePageDirSlots>: Unsigned,
    {
        let unmapped_scratch_page_table: LocalCap<UnmappedPageTable> = cap::retype(ut, slot)?;
        let (page_table, page_directory) =
            page_directory.map_page_table(unmapped_scratch_page_table)?;

        Ok((
            VSpaceScratchSlice {
                page_table,
                page_dir: Cap {
                    cptr: page_directory.cptr,
                    _role: PhantomData,
                    cap_data: AssignedPageDirectory {
                        next_free_slot: core::usize::MAX,
                        _free_slots: PhantomData,
                        _role: PhantomData,
                    },
                },
            },
            page_directory,
        ))
    }

    pub fn temporarily_map_pages<F, Out, Kind: MemoryKind, PageCount: Unsigned>(
        &mut self,
        unmapped_pages: CapRange<UnmappedPage<Kind>, role::Local, PageCount>,
        f: F,
    ) -> Result<(Out, CapRange<UnmappedPage<Kind>, role::Local, PageCount>), SeL4Error>
    where
        F: Fn(&GenericArray<LocalCap<MappedPage<role::Local, Kind>>, PageCount>) -> Out,
        ScratchPageTableSlots: IsGreaterOrEqual<PageCount, Output = True>,
        PageCount: ArrayLength<LocalCap<MappedPage<role::Local, Kind>>>,
    {
        self.page_table
            .temporarily_map_pages(unmapped_pages, &mut self.page_dir, f)
    }

    pub fn temporarily_map_page<F, Out, Kind: MemoryKind>(
        &mut self,
        unmapped_page: LocalCap<UnmappedPage<Kind>>,
        f: F,
    ) -> Result<(Out, LocalCap<UnmappedPage<Kind>>), SeL4Error>
    where
        F: Fn(&LocalCap<MappedPage<role::Local, Kind>>) -> Out,
        ScratchPageTableSlots: IsGreaterOrEqual<U1, Output = True>,
    {
        self.page_table
            .temporarily_map_page(unmapped_page, &mut self.page_dir, f)
    }
}
