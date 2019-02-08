use core::marker::PhantomData;
use core::mem::size_of;
use core::ops::Sub;
use crate::pow::Pow;
use crate::userland::cap::UnassignedPageDirectory;
use crate::userland::{
    memory_kind, role, ASIDControl, ASIDPool, AssignedPageDirectory, CNode, Cap, IRQControl,
    LocalCap, MappedPage, MappedPageTable, SeL4Error, ThreadControlBlock, UnmappedPageTable,
    Untyped,
};
use sel4_sys::*;
use typenum::operator_aliases::Sub1;
use typenum::{Unsigned, B1, U1024, U12};

// TODO: how many slots are there really? We should be able to know this at build
// time.
// Answer: The radix is 19, and there are 12 initial caps. But there are also a bunch
// of random things in the bootinfo.
// TODO: ideally, this should only be callable once in the process. Is that possible?
pub fn root_cnode(_bootinfo: &'static seL4_BootInfo) -> LocalCap<CNode<U1024, role::Local>> {
    Cap {
        cptr: seL4_CapInitThreadCNode as usize,
        _role: PhantomData,
        cap_data: CNode {
            radix: 19,
            next_free_slot: 1000, // TODO: look at the bootinfo to determine the real value
            _free_slots: PhantomData,
            _role: PhantomData,
        },
    }
}

pub mod paging {
    use crate::pow::Pow;
    use typenum::operator_aliases::Diff;
    use typenum::{U1, U1024, U12, U8, U9};

    pub type BaseASIDPoolFreeSlots = U1024;
    pub type PageDirectoryBits = U12;
    pub type PageTableBits = U8;
    pub type PageBits = U12;

    // 0xe00000000 and up is reserved to the kernel; this translates to the last
    // 2^9 (512) pagedir entries.
    pub type BasePageDirFreeSlots = Diff<Pow<PageDirectoryBits>, Pow<U9>>;

    pub type BasePageTableFreeSlots = Pow<PageTableBits>;

    // The first page table is already mapped for the root task, for the user
    // image. (which also reserves 64k for the root task's stack)
    pub type RootTaskReservedPageDirSlots = U1;

    pub type RootTaskPageDirFreeSlots = Diff<BasePageDirFreeSlots, RootTaskReservedPageDirSlots>;

    // Useful for constant comparison to data structure size_of results
    pub type PageBytes = Pow<PageBits>;
}

pub mod address_space {
    use crate::pow::Pow;
    use typenum::operator_aliases::Sum;
    use typenum::{U0, U16, U20, U29, U30, U31};

    // TODO this is a magic numbers we got from inspecting the binary.
    /// 0x00010000
    pub type ProgramStart = Pow<U16>;

    pub type ProgramStartPageTableSlot = U0;

    // TODO calculate the real one
    /// 0x00080000 - the end of the range of the first page table
    pub type ProgramEnd = Pow<U20>;

    /// 0xe0000000
    pub type KernelReservedStart = Sum<Pow<U31>, Sum<Pow<U30>, Pow<U29>>>;
}

/// Currently assume that a BootInfo cannot be handed to child processes
/// and thus its related structures always operate in a "Local" role.
pub struct BootInfo<ASIDPoolFreeSlots: Unsigned, PageDirFreeSlots: Unsigned> {
    pub page_directory: LocalCap<AssignedPageDirectory<PageDirFreeSlots, role::Local>>,
    pub tcb: LocalCap<ThreadControlBlock>,
    pub asid_pool: LocalCap<ASIDPool<ASIDPoolFreeSlots>>,
    pub irq_control: LocalCap<IRQControl>,
    user_image_frames_start: usize,
    user_image_frames_end: usize,
}

impl<ASIDPoolFreeSlots: Unsigned, PageDirFreeSlots: Unsigned> !Send
    for BootInfo<ASIDPoolFreeSlots, PageDirFreeSlots>
{
}

impl BootInfo<paging::BaseASIDPoolFreeSlots, paging::RootTaskPageDirFreeSlots> {
    pub fn wrap<FreeSlots: Unsigned>(
        bootinfo: &'static seL4_BootInfo,
        asid_pool_ut: LocalCap<Untyped<U12>>,
        dest_cnode: LocalCap<CNode<FreeSlots, role::Local>>,
    ) -> (Self, LocalCap<CNode<Sub1<FreeSlots>, role::Local>>)
    where
        FreeSlots: Sub<B1>,
        Sub1<FreeSlots>: Unsigned,
    {
        let asid_control = Cap::wrap_cptr(seL4_CapASIDControl as usize);
        let (asid_pool, dest_cnode): (Cap<ASIDPool<_>, _>, _) = asid_pool_ut
            .retype_asid_pool(asid_control, dest_cnode)
            .expect("retype asid pool");

        (
            BootInfo {
                page_directory: Cap {
                    cptr: seL4_CapInitThreadVSpace as usize,
                    _role: PhantomData,
                    cap_data: AssignedPageDirectory {
                        next_free_slot: paging::RootTaskReservedPageDirSlots::USIZE,
                        _free_slots: PhantomData,
                        _role: PhantomData,
                    },
                },
                tcb: Cap::wrap_cptr(seL4_CapInitThreadTCB as usize),
                asid_pool,
                irq_control: Cap {
                    cptr: seL4_CapIRQControl as usize,
                    cap_data: IRQControl {
                        known_handled: [false; 256],
                    },
                    _role: PhantomData,
                },
                user_image_frames_start: bootinfo.userImageFrames.start,
                user_image_frames_end: bootinfo.userImageFrames.end,
            },
            dest_cnode,
        )
    }
}

impl<ASIDPoolFreeSlots: Unsigned, PageDirFreeSlots: Unsigned>
    BootInfo<ASIDPoolFreeSlots, PageDirFreeSlots>
{
    // TODO this doesn't enforce the aliasing constraints we want at the type
    // level. This can be modeled as an array (or other sized thing) once we
    // know how big the user image is.
    pub fn user_image_pages_iter(
        &self,
    ) -> impl Iterator<Item = LocalCap<MappedPage<role::Local, memory_kind::General>>> {
        let vaddr_iter = (address_space::ProgramStart::USIZE..address_space::ProgramEnd::USIZE)
            .step_by(1 << paging::PageBits::USIZE);

        (self.user_image_frames_start..self.user_image_frames_end)
            .zip(vaddr_iter)
            .map(|(cptr, vaddr)| Cap {
                cptr,
                cap_data: MappedPage {
                    vaddr,
                    _role: PhantomData,
                    _kind: PhantomData,
                },
                _role: PhantomData,
            })
    }

    /// Proxy to page_directory for convenience
    pub fn map_page_table(
        self,
        unmapped_page_table: LocalCap<UnmappedPageTable>,
    ) -> Result<
        (
            LocalCap<MappedPageTable<Pow<paging::PageTableBits>, role::Local>>,
            BootInfo<ASIDPoolFreeSlots, Sub1<PageDirFreeSlots>>,
        ),
        SeL4Error,
    >
    where
        PageDirFreeSlots: Sub<B1>,
        Sub1<PageDirFreeSlots>: Unsigned,
    {
        let (mapped_page_table, page_dir) =
            self.page_directory.map_page_table(unmapped_page_table)?;
        Ok((
            mapped_page_table,
            BootInfo {
                page_directory: page_dir,
                tcb: self.tcb,
                asid_pool: self.asid_pool,
                irq_control: self.irq_control,
                user_image_frames_start: self.user_image_frames_start,
                user_image_frames_end: self.user_image_frames_end,
            },
        ))
    }

    /// Convenience wrapper allowing assignment of page dirs to the
    /// ASID Pool while updating the type signature appropriately,
    /// saving the caller from having to de/re-structure BootInfo
    pub fn assign_minimal_page_dir(
        self,
        page_dir: LocalCap<UnassignedPageDirectory>,
    ) -> Result<
        (
            LocalCap<AssignedPageDirectory<paging::BasePageDirFreeSlots, role::Child>>,
            BootInfo<Sub1<ASIDPoolFreeSlots>, PageDirFreeSlots>,
        ),
        SeL4Error,
    >
    where
        ASIDPoolFreeSlots: Sub<B1>,
        Sub1<ASIDPoolFreeSlots>: Unsigned,
    {
        let (assigned_page_dir, asid_pool) = self.asid_pool.assign_minimal(page_dir)?;
        Ok((
            assigned_page_dir,
            BootInfo {
                page_directory: self.page_directory,
                tcb: self.tcb,
                asid_pool: asid_pool,
                irq_control: self.irq_control,
                user_image_frames_start: self.user_image_frames_start,
                user_image_frames_end: self.user_image_frames_end,
            },
        ))
    }
}

/// The ASID pool needs an untyped of exactly 4k.
///
/// Note that we fully consume the ASIDControl capability,
/// (which is assumed to be a singleton as well)
/// because there is a lightly-documented seL4 constraint
/// that limits us to a single ASIDPool per application.
impl LocalCap<Untyped<U12, memory_kind::General>> {
    pub fn retype_asid_pool<FreeSlots: Unsigned>(
        self,
        asid_control: LocalCap<ASIDControl>,
        dest_cnode: LocalCap<CNode<FreeSlots, role::Local>>,
    ) -> Result<
        (
            LocalCap<ASIDPool<paging::BaseASIDPoolFreeSlots>>,
            LocalCap<CNode<Sub1<FreeSlots>, role::Local>>,
        ),
        SeL4Error,
    >
    where
        FreeSlots: Sub<B1>,
        Sub1<FreeSlots>: Unsigned,
    {
        let (dest_cnode, dest_slot) = dest_cnode.consume_slot();

        let err = unsafe {
            seL4_ARM_ASIDControl_MakePool(
                asid_control.cptr,              // _service
                self.cptr,                      // untyped
                dest_slot.cptr,                 // root
                dest_slot.offset,               // index
                (8 * size_of::<usize>()) as u8, // depth
            )
        };

        if err != 0 {
            return Err(SeL4Error::UntypedRetype(err));
        }

        Ok((
            Cap {
                cptr: dest_slot.offset,
                cap_data: ASIDPool {
                    next_free_slot: 0,
                    _free_slots: PhantomData,
                },
                _role: PhantomData,
            },
            dest_cnode,
        ))
    }
}
