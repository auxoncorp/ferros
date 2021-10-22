use super::TopLevelError;

use ferros::alloc::{smart_alloc, ut_buddy};
use typenum::*;

use elf_process;
use ferros::bootstrap::UserImage;
use ferros::cap::*;
use ferros::userland::{fault_or_message_channel, FaultOrMessage, StandardProcess};
use ferros::vspace::*;
use selfe_arc;

#[ferros_test::ferros_test]
pub fn weak_elf_process_runs<'a, 'b, 'c>(
    local_slots: LocalCNodeSlots<U32768>,
    local_ut: LocalCap<Untyped<U20>>,
    asid_pool: LocalCap<ASIDPool<U1>>,
    stack_mem: MappedMemoryRegion<U17, shared_status::Exclusive>,
    root_cnode: &LocalCap<LocalCNode>,
    user_image: &UserImage<role::Local>,
    tpa: &LocalCap<ThreadPriorityAuthority>,
    mut local_vspace_scratch: &mut ScratchRegion,
) -> Result<(), TopLevelError> {
    let uts = ut_buddy(local_ut);

    let archive_slice: &[u8] = unsafe {
        core::slice::from_raw_parts(
            &crate::_selfe_arc_data_start,
            &crate::_selfe_arc_data_end as *const _ as usize
                - &crate::_selfe_arc_data_start as *const _ as usize,
        )
    };

    let archive = selfe_arc::read::Archive::from_slice(archive_slice);
    let elf_data = archive
        .file(crate::resources::ElfProcess::IMAGE_NAME)
        .expect("find elf-process in arc");

    smart_alloc!(|slots: local_slots, ut: uts| {
        let (child_cnode, child_slots) = retype_cnode::<U12>(ut, slots)?;
        let (child_fault_source_slot, _child_slots) = child_slots.alloc();
        let (fault_source, outcome_sender, handler) =
            fault_or_message_channel(&root_cnode, ut, slots, child_fault_source_slot, slots)?;

        let params: elf_process::ProcParams<role::Child> = elf_process::ProcParams {
            value: 42,
            outcome_sender,
        };

        let child_root = retype(ut, slots)?;
        let child_vspace_slots: LocalCNodeSlots<U1024> = slots;
        let child_vspace_ut: LocalCap<Untyped<U15>> = ut;
        let (child_asid, _asid_pool) = asid_pool.alloc();

        let page_slots: LocalCNodeSlots<U1024> = slots;
        let writable_mem: LocalCap<Untyped<U18>> = ut;

        let mut child_vspace = VSpace::new_from_elf_weak(
            child_root,
            child_asid,
            child_vspace_slots.weaken(),
            child_vspace_ut.weaken(),
            &elf_data,
            page_slots.weaken(),
            writable_mem.weaken(),
            &user_image,
            &root_cnode,
            &mut local_vspace_scratch,
        )?;

        let mut child_process = StandardProcess::new::<elf_process::ProcParams<_>, _>(
            &mut child_vspace,
            child_cnode,
            stack_mem,
            root_cnode,
            elf_data,
            params,
            ut, // ipc_buffer_ut
            ut, // tcb_ut
            slots,
            tpa,  // priority_authority
            None, // fault
        )?;
    });

    child_process.start()?;

    match handler.await_message()? {
        FaultOrMessage::Message(true) => Ok(()),
        _ => Err(TopLevelError::TestAssertionFailure(
            "Child process should have reported success",
        )),
    }
}
