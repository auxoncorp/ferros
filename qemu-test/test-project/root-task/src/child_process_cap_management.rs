use super::TopLevelError;

use typenum::*;

use ferros::alloc::{smart_alloc, ut_buddy};
use ferros::bootstrap::UserImage;
use ferros::cap::*;
use ferros::userland::{
    fault_or_message_channel, FaultOrMessage, RetypeForSetup, Sender, StandardProcess,
};
use ferros::vspace::*;

#[ferros_test::ferros_test]
pub fn child_process_cap_management(
    local_slots: LocalCNodeSlots<U32768>,
    local_ut: LocalCap<Untyped<U20>>,
    asid_pool: LocalCap<ASIDPool<U1>>,
    local_mapped_region: MappedMemoryRegion<U17, shared_status::Exclusive>,
    root_cnode: &LocalCap<LocalCNode>,
    user_image: &UserImage<role::Local>,
    tpa: &LocalCap<ThreadPriorityAuthority>,
) -> Result<(), TopLevelError> {
    let uts = ut_buddy(local_ut);

    smart_alloc!(|slots: local_slots, ut: uts| {
        let (child_asid, _asid_pool) = asid_pool.alloc();
        let (child_cnode, child_slots) = retype_cnode::<U12>(ut, slots)?;

        let ut5: LocalCap<Untyped<U5>> = ut;

        let child_root = retype(ut, slots)?;
        let child_vspace_slots: LocalCNodeSlots<U1024> = slots;
        let child_vspace_ut: LocalCap<Untyped<U15>> = ut;

        smart_alloc! {|slots_c: child_slots| {
            let (cnode_for_child, slots_for_child) =
                child_cnode.generate_self_reference(&root_cnode, slots_c)?;
            let child_ut5 = ut5.move_to_slot(&root_cnode, slots_c)?;
            let (fault_source, outcome_sender, handler) = fault_or_message_channel(
                &root_cnode,
                ut,
                slots,
                slots_c,
                slots,
            )?;
        }}

        let params = CapManagementParams {
            my_cnode: cnode_for_child,
            my_cnode_slots: slots_for_child,
            my_ut: child_ut5,
            outcome_sender,
        };

        let mut child_vspace = VSpace::new(
            child_root,
            child_asid,
            child_vspace_slots.weaken(),
            child_vspace_ut.weaken(),
            ProcessCodeImageConfig::ReadOnly,
            user_image,
            root_cnode,
        )?;

        let mut child_process = StandardProcess::new(
            &mut child_vspace,
            child_cnode,
            local_mapped_region,
            root_cnode,
            proc_main as extern "C" fn(_) -> (),
            params,
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

#[derive(Debug)]
pub struct CapManagementParams<Role: CNodeRole> {
    pub my_cnode: Cap<CNode<Role>, Role>,
    pub my_cnode_slots: Cap<CNodeSlotsData<U42, Role>, Role>,
    pub my_ut: Cap<Untyped<U5>, Role>,
    pub outcome_sender: Sender<bool, Role>,
}

impl RetypeForSetup for CapManagementParams<role::Local> {
    type Output = CapManagementParams<role::Child>;
}

// 'extern' to force C calling conventions
pub extern "C" fn proc_main(params: CapManagementParams<role::Local>) {
    let CapManagementParams {
        my_cnode,
        my_cnode_slots,
        my_ut,
        outcome_sender,
    } = params;

    smart_alloc!(|slots: my_cnode_slots| {
        let (ut_kid_a, ut_kid_b) = my_ut.split(slots).expect("child process split untyped");
        let _endpoint: LocalCap<Endpoint> =
            retype(ut_kid_a, slots).expect("Retype local in a child process failure");
        ut_kid_b
            .delete(&my_cnode)
            .expect("child process delete a cap");
    });
    outcome_sender
        .blocking_send(&true)
        .expect("Failed to report cap management success")
}
