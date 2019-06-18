use super::TopLevelError;

use ferros::alloc::{smart_alloc, ut_buddy};
use typenum::*;

use ferros::bootstrap::UserImage;
use ferros::cap::*;
use ferros::userland::{
    fault_or_message_channel, FaultOrMessage, ReadyProcess, RetypeForSetup, Sender,
};
use ferros::vspace::{ProcessCodeImageConfig, ScratchRegion, VSpace};

#[ferros_test::ferros_test]
pub fn child_process_runs(
    local_slots: LocalCNodeSlots<U32768>,
    local_ut: LocalCap<Untyped<U20>>,
    asid_pool: LocalCap<ASIDPool<U1>>,
    local_vspace_scratch: &mut ScratchRegion,
    root_cnode: &LocalCap<LocalCNode>,
    user_image: &UserImage<role::Local>,
    tpa: &LocalCap<ThreadPriorityAuthority>,
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

        let (child_asid, _asid_pool) = asid_pool.alloc();

        let child_root = retype(ut, slots)?;
        let child_vspace_slots: LocalCNodeSlots<U1024> = slots;
        let child_vspace_ut: LocalCap<Untyped<U14>> = ut;

        let mut child_vspace = VSpace::new(
            child_root,
            child_asid,
            child_vspace_slots.weaken(),
            child_vspace_ut.weaken(),
            ProcessCodeImageConfig::ReadOnly,
            user_image,
            root_cnode,
        )?;

        let child_process = ReadyProcess::new(
            &mut child_vspace,
            child_cnode,
            local_vspace_scratch,
            proc_main,
            params,
            ut,
            ut,
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
