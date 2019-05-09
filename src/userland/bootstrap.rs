use crate::arch::{address_space, asid, paging};
use crate::pow::Pow;
use crate::userland::process::NeitherSendNorSync;
use crate::userland::{
    memory_kind, role, ASIDControl, AssignedPageDirectory, CNode, CNodeRole, CNodeSlots, Cap,
    CapRights, IRQControl, LocalCNode, LocalCNodeSlots, LocalCap, MappedPage, SeL4Error,
    ThreadControlBlock,
};
use core::marker::PhantomData;
use selfe_sys::*;
use typenum::operator_aliases::Diff;
use typenum::*;

// The root CNode radix is 19. Conservatively set aside 2^12 (the default root
// cnode size) for system use. TODO: verify at build time that this is enough /
// compute a better number
type RootCNodeSize = Pow<U19>;
type SystemProvidedCapCount = Pow<U12>;
type RootCNodeAvailableSlots = Diff<RootCNodeSize, SystemProvidedCapCount>;

// of random things in the bootinfo.
// TODO: ideally, this should only be callable once in the process. Is that possible?
pub fn root_cnode(
    bootinfo: &'static seL4_BootInfo,
) -> (
    LocalCap<LocalCNode>,
    LocalCNodeSlots<RootCNodeAvailableSlots>,
) {
    (
        Cap {
            cptr: seL4_CapInitThreadCNode as usize,
            _role: PhantomData,
            cap_data: CNode {
                radix: 19,
                _role: PhantomData,
            },
        },
        CNodeSlots::internal_new(seL4_CapInitThreadCNode as usize, bootinfo.empty.start),
    )
}

/// Encapsulate the user image information found in bootinfo
///
/// This is very similar to a more dynamic CapRange, but presently distinct
/// in that the related cap-type we want to iterate over (MappedPage)
/// bears associated cap-data (namely the mapped virtual address).
///
/// Additionally, and importantly, the number of capabilities is not
/// known until runtime and thus not represented in the type.
#[derive(Debug)]
pub struct UserImage<Role: CNodeRole> {
    frames_start_cptr: usize,
    frames_count: usize,
    page_table_count: usize,
    _role: PhantomData<Role>,
}

/// A BootInfo cannot be handed to child processes and thus its related
/// structures always operate in a "Local" role.
pub struct BootInfo<ASIDControlFreePools: Unsigned, PageDirFreeSlots: Unsigned> {
    pub root_page_directory: LocalCap<AssignedPageDirectory<PageDirFreeSlots, role::Local>>,
    pub root_tcb: LocalCap<ThreadControlBlock>,

    pub asid_control: LocalCap<ASIDControl<ASIDControlFreePools>>,
    pub irq_control: LocalCap<IRQControl>,
    pub user_image: UserImage<role::Local>,

    #[allow(dead_code)]
    neither_send_nor_sync: NeitherSendNorSync,
}

impl BootInfo<asid::PoolCount, paging::RootTaskPageDirFreeSlots> {
    pub fn wrap(bootinfo: &'static seL4_BootInfo) -> Self {
        let asid_control = Cap::wrap_cptr(seL4_CapASIDControl as usize);

        BootInfo {
            root_page_directory: Cap {
                cptr: seL4_CapInitThreadVSpace as usize,
                _role: PhantomData,
                cap_data: AssignedPageDirectory {
                    next_free_slot: paging::RootTaskReservedPageDirSlots::USIZE,
                    _free_slots: PhantomData,
                    _role: PhantomData,
                },
            },
            root_tcb: Cap::wrap_cptr(seL4_CapInitThreadTCB as usize),
            asid_control,
            irq_control: Cap {
                cptr: seL4_CapIRQControl as usize,
                cap_data: IRQControl {
                    known_handled: [false; 256],
                },
                _role: PhantomData,
            },
            user_image: UserImage {
                frames_start_cptr: bootinfo.userImageFrames.start,
                frames_count: bootinfo.userImageFrames.end - bootinfo.userImageFrames.start,
                page_table_count: bootinfo.userImagePaging.end - bootinfo.userImagePaging.start,
                _role: PhantomData,
            },

            neither_send_nor_sync: Default::default(),
        }
    }
}

impl UserImage<role::Local> {
    pub fn page_table_count(&self) -> usize {
        self.page_table_count
    }

    // TODO this doesn't enforce the aliasing constraints we want at the type
    // level. This can be modeled as an array (or other sized thing) once we
    // know how big the user image is.
    pub fn pages_iter(
        &self,
    ) -> impl Iterator<Item = LocalCap<MappedPage<role::Local, memory_kind::General>>> {
        // Iterate over the entire address space's page addresses, starting at
        // ProgramStart. This is truncated to the number of actual pages in the
        // user image by zipping it with the range of frame cptrs below.
        let vaddr_iter = (address_space::ProgramStart::USIZE..core::usize::MAX)
            .step_by(1 << paging::PageBits::USIZE);

        (self.frames_start_cptr..(self.frames_start_cptr + self.frames_count))
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

    pub fn copy<TargetRole: CNodeRole>(
        &self,
        src_cnode: &LocalCap<LocalCNode>,
        slots: CNodeSlots<paging::CodePageCount, TargetRole>,
    ) -> Result<UserImage<TargetRole>, SeL4Error> {
        let frames_start_cptr = slots.cap_data.offset;
        for (page, slot) in self.pages_iter().zip(slots.iter()) {
            let _ = page.copy(src_cnode, slot, CapRights::RWG)?;
        }

        Ok(UserImage {
            frames_start_cptr,
            frames_count: self.frames_count,
            page_table_count: self.page_table_count,
            _role: PhantomData,
        })
    }
}
