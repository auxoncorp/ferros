use super::TopLevelError;

use ferros::alloc::{smart_alloc, ut_buddy};
use typenum::*;

use ferros::bootstrap::UserImage;
use ferros::cap::*;
use ferros::userland::{fault_or_message_channel, FaultOrMessage, RetypeForSetup, Sender, Thread};
use ferros::vspace::*;

#[ferros_test::ferros_test]
pub fn child_thread_runs(
    local_slots: LocalCNodeSlots<U32768>,
    local_ut: LocalCap<Untyped<U20>>,
    asid_pool: LocalCap<ASIDPool<U1>>,
    stack_mapped_region: MappedMemoryRegion<U17, shared_status::Exclusive>,
    ipc_buffer_region: MappedMemoryRegion<U12, shared_status::Exclusive>,
    root_cnode: &LocalCap<LocalCNode>,
    user_image: &UserImage<role::Local>,
    tpa: &LocalCap<ThreadPriorityAuthority>,
    vspace_paging_root: &LocalCap<ferros::arch::PagingRoot>,
) -> Result<(), TopLevelError> {
    let uts = ut_buddy(local_ut);

    smart_alloc!(|slots: local_slots, ut: uts| {
        let (child_cnode, child_slots) = retype_cnode::<U12>(ut, slots)?;
        let (child_fault_source_slot, _child_slots) = child_slots.alloc();
        let (fault_source, outcome_sender, handler) =
            fault_or_message_channel(&root_cnode, ut, slots, child_fault_source_slot, slots)?;
        let params = ProcParams {
            value: 42,
            outcome_sender,
        };

        let child_process = Thread::new(
            vspace_paging_root,
            child_cnode,
            stack_mapped_region,
            proc_main,
            params,
            ipc_buffer_region,
            ut,
            slots,
            tpa,
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

pub struct ProcParams<Role: CNodeRole> {
    pub value: usize,
    pub outcome_sender: Sender<bool, Role>,
}

impl RetypeForSetup for ProcParams<role::Local> {
    type Output = ProcParams<role::Child>;
}

pub extern "C" fn proc_main(params: ProcParams<role::Local>) {
    params
        .outcome_sender
        .blocking_send(&(params.value == 42))
        .expect("Found value does not match expectations")
}
