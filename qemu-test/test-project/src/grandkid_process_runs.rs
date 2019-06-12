use typenum::*;

use ferros::alloc::{smart_alloc, ut_buddy};
use ferros::bootstrap::UserImage;
use ferros::cap::*;
use ferros::userland::{
    fault_or_message_channel, CapRights, FaultOrMessage, RetypeForSetup, Sender,
};
use ferros::vspace::{NewVSpaceCNodeSlots, VSpace, VSpaceScratchSlice};

use super::TopLevelError;

type U33768 = Sum<U32768, U1000>;

#[ferros_test::ferros_test]
pub fn grandkid_process_runs(
    local_slots: LocalCNodeSlots<U33768>,
    local_ut: LocalCap<Untyped<U27>>,
    asid_pool: LocalCap<ASIDPool<U6>>,
    local_vspace_scratch: &mut VSpaceScratchSlice<role::Local>,
    cnode: &LocalCap<LocalCNode>,
    user_image: &UserImage<role::Local>,
    tpa: &LocalCap<ThreadPriorityAuthority>,
) -> Result<(), TopLevelError> {
    let uts = ut_buddy(local_ut);

    smart_alloc!(|slots: local_slots, ut: uts| {
        let (child_cnode, child_slots) = retype_cnode::<U20>(ut, slots)?;

        let (child_asid, asid_pool) = asid_pool.alloc();
        let child_vspace = VSpace::new(ut, slots, child_asid, &user_image, &cnode)?;

        smart_alloc! {|slots_c: child_slots| {
            let (cnode_for_child, slots_for_child) =
                child_cnode.generate_self_reference(&cnode, slots_c)?;
            let untyped_for_child = ut.move_to_slot(&cnode, slots_c)?;
            let (asid_pool_for_child, _asid_pool): (_, LocalCap<ASIDPool<U0>>) = asid_pool.split(slots_c, slots, &cnode)?;
            let user_image_for_child = user_image.copy(&cnode, slots_c)?;
            let (vspace_scratch_for_child, child_vspace) = child_vspace.create_child_scratch(
                ut,
                slots,
                slots_c,
                &cnode,
            )?;
            let thread_priority_authority_for_child =
                tpa.copy(&cnode, slots_c, CapRights::RWG)?;

            let (fault_source, outcome_sender, handler) = fault_or_message_channel(
                &cnode,
                ut,
                slots,
                slots_c,
                slots,
            )?;
        }}

        let params = ChildParams {
            cnode: cnode_for_child,
            cnode_slots: slots_for_child,
            untyped: untyped_for_child,
            asid_pool: asid_pool_for_child,
            user_image: user_image_for_child,
            vspace_scratch: vspace_scratch_for_child,
            thread_priority_authority: thread_priority_authority_for_child,
            outcome_sender,
        };

        let (child_process, _) =
            child_vspace.prepare_thread(child_main, params, ut, slots, local_vspace_scratch)?;
    });

    child_process.start(child_cnode, Some(fault_source), tpa, 255)?;

    match handler.await_message()? {
        FaultOrMessage::Message(true) => Ok(()),
        _ => Err(TopLevelError::TestAssertionFailure(
            "Child process should have reported success",
        )),
    }
}

#[derive(Debug)]
pub struct ChildParams<Role: CNodeRole> {
    cnode: Cap<CNode<Role>, Role>,
    cnode_slots: Cap<CNodeSlotsData<Sum<NewVSpaceCNodeSlots, U70>, Role>, Role>,
    untyped: Cap<Untyped<U25>, Role>,
    asid_pool: Cap<ASIDPool<U2>, Role>,
    user_image: UserImage<Role>,
    vspace_scratch: VSpaceScratchSlice<Role>,
    thread_priority_authority: Cap<ThreadPriorityAuthority, Role>,
    outcome_sender: Sender<bool, Role>,
}

impl RetypeForSetup for ChildParams<role::Local> {
    type Output = ChildParams<role::Child>;
}

pub extern "C" fn child_main(params: ChildParams<role::Local>) {
    child_run(params).expect("Error in child process");
}

fn child_run(params: ChildParams<role::Local>) -> Result<(), TopLevelError> {
    let ChildParams {
        cnode,
        cnode_slots,
        untyped,
        asid_pool,
        user_image,
        mut vspace_scratch,
        thread_priority_authority,
        outcome_sender,
    } = params;
    let uts = ut_buddy(untyped);

    smart_alloc!(|slots: cnode_slots, ut: uts| {
        let (child_cnode, child_slots) = retype_cnode::<U8>(ut, slots)?;
        let (outcome_sender_slot, _child_slots) = child_slots.alloc();
        let params = GrandkidParams {
            outcome_sender: outcome_sender.copy(&cnode, outcome_sender_slot)?,
        };

        let (child_asid, _asid_pool) = asid_pool.alloc();
        let child_vspace = VSpace::new(ut, slots, child_asid, &user_image, &cnode)?;
        let (child_process, _) =
            child_vspace.prepare_thread(grandkid_main, params, ut, slots, &mut vspace_scratch)?;
    });
    child_process.start(child_cnode, None, &thread_priority_authority, 255)?;

    Ok(())
}

pub struct GrandkidParams<Role: CNodeRole> {
    pub outcome_sender: Sender<bool, Role>,
}

impl RetypeForSetup for GrandkidParams<role::Local> {
    type Output = GrandkidParams<role::Child>;
}

pub extern "C" fn grandkid_main(params: GrandkidParams<role::Local>) {
    params
        .outcome_sender
        .blocking_send(&true)
        .expect("failed to send test outcome");
}
