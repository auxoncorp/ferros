use typenum::*;

use ferros::alloc::{smart_alloc, ut_buddy};
use ferros::arch::{self, CodePageCount, CodePageTableCount, PageBytes};
use ferros::bootstrap::UserImage;
use ferros::cap::*;
use ferros::userland::{
    fault_or_message_channel, CapRights, FaultOrMessage, RetypeForSetup, Sender, StandardProcess,
};
use ferros::vspace::{
    shared_status, MappedMemoryRegion, ProcessCodeImageConfig, UnmappedMemoryRegion, VSpace,
};

use super::TopLevelError;

type U33768 = Sum<U32768, U1000>;

#[ferros_test::ferros_test]
pub fn grandkid_process_runs(
    local_slots: LocalCNodeSlots<U33768>,
    local_ut: LocalCap<Untyped<U27>>,
    asid_pool: LocalCap<ASIDPool<U6>>,
    local_mapped_region: MappedMemoryRegion<U17, shared_status::Exclusive>,
    cnode: &LocalCap<LocalCNode>,
    user_image: &UserImage<role::Local>,
    tpa: &LocalCap<ThreadPriorityAuthority>,
) -> Result<(), TopLevelError> {
    let uts = ut_buddy(local_ut);

    smart_alloc!(|slots: local_slots, ut: uts| {
        let (child_cnode, child_slots) = retype_cnode::<U20>(ut, slots)?;

        let (child_asid, asid_pool) = asid_pool.alloc();
        let child_root = retype(ut, slots)?;
        let child_vspace_slots: LocalCNodeSlots<U1024> = slots;
        // NOTE: this needs to be big enough to map in entire root task. That
        // could grow if you add more tests elsewhere.
        let child_vspace_ut: LocalCap<Untyped<U16>> = ut;

        let mut child_vspace = VSpace::new(
            child_root,
            child_asid,
            child_vspace_slots.weaken(),
            child_vspace_ut.weaken(),
            ProcessCodeImageConfig::ReadOnly,
            user_image,
            cnode,
        )?;

        smart_alloc! {|slots_c: child_slots| {
            let (cnode_for_child, slots_for_child) =
                child_cnode.generate_self_reference(&cnode, slots_c)?;
            let untyped_for_child = ut.move_to_slot(&cnode, slots_c)?;
            let (asid_pool_for_child, _asid_pool): (_, LocalCap<ASIDPool<U0>>) = asid_pool.split(slots_c, slots, &cnode)?;
            let user_image_for_child = user_image.copy(&cnode, slots_c)?;
            let thread_priority_authority_for_child =
                tpa.copy(&cnode, slots_c, CapRights::RWG)?;

            let (fault_source, outcome_sender, handler) = fault_or_message_channel(
                &cnode,
                ut,
                slots,
                slots_c,
                slots,
            )?;

            let child_unmapped_region: UnmappedMemoryRegion<U17, shared_status::Exclusive> =
                UnmappedMemoryRegion::new(ut, slots)?;
            let child_mapped_region = child_vspace.map_region_and_move(
                child_unmapped_region,
                CapRights::RW,
                arch::vm_attributes::DEFAULT,
                cnode,
                slots_c,
            )?;
        }}

        let params = ChildParams {
            cnode: cnode_for_child,
            cnode_slots: slots_for_child,
            untyped: untyped_for_child,
            asid_pool: asid_pool_for_child,
            user_image: user_image_for_child,
            mapped_region: child_mapped_region,
            thread_priority_authority: thread_priority_authority_for_child,
            outcome_sender,
        };

        let mut child_process = StandardProcess::new(
            &mut child_vspace,
            child_cnode,
            local_mapped_region,
            &cnode,
            child_main as extern "C" fn(_) -> (),
            params,
            ut,
            ut,
            slots,
            tpa,
            Some(fault_source),
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

pub struct ChildParams<Role: CNodeRole> {
    cnode: Cap<CNode<Role>, Role>,
    cnode_slots: Cap<CNodeSlotsData<op!(CodePageTableCount + CodePageCount + U70), Role>, Role>,
    untyped: Cap<Untyped<U25>, Role>,
    asid_pool: Cap<ASIDPool<U2>, Role>,
    user_image: UserImage<Role>,
    mapped_region: MappedMemoryRegion<U17, shared_status::Exclusive>,
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
        mut cnode,
        cnode_slots,
        untyped,
        asid_pool,
        user_image,
        mapped_region,
        thread_priority_authority,
        outcome_sender,
    } = params;

    let uts = ut_buddy(untyped);

    // Clean/invalidate the entire region
    mapped_region.flush();
    mapped_region
        .flush_range(mapped_region.vaddr(), mapped_region.size_bytes())
        .unwrap();

    // Clean/invalidate by page
    let range = mapped_region.vaddr()..mapped_region.vaddr() + mapped_region.size_bytes();
    for vaddr in range.step_by(PageBytes::USIZE) {
        mapped_region.flush_range(vaddr, PageBytes::USIZE).unwrap();
    }

    // Clean/invalidate arbitrary ranges
    let first = mapped_region.vaddr();
    let mid = mapped_region.vaddr() + (mapped_region.size_bytes() / 2);
    mapped_region.flush_range(first, mid - first).unwrap();
    mapped_region
        .flush_range(
            mid,
            mapped_region.size_bytes() - (mapped_region.size_bytes() / 2),
        )
        .unwrap();

    smart_alloc!(|slots: cnode_slots, ut: uts| {
        let (child_cnode, child_slots) = retype_cnode::<U8>(ut, slots)?;
        let (outcome_sender_slot, _child_slots) = child_slots.alloc();
        let params = GrandkidParams {
            outcome_sender: outcome_sender.copy(&cnode, outcome_sender_slot)?,
        };

        let (child_asid, _asid_pool) = asid_pool.alloc();
        let child_root = retype(ut, slots)?;
        let child_vspace_slots: LocalCNodeSlots<U1024> = slots;
        let child_vspace_ut: LocalCap<Untyped<U15>> = ut;

        let mut child_vspace = VSpace::new(
            child_root,
            child_asid,
            child_vspace_slots.weaken(),
            child_vspace_ut.weaken(),
            ProcessCodeImageConfig::ReadOnly,
            &user_image,
            &cnode,
        )?;

        let mut child_process = StandardProcess::new(
            &mut child_vspace,
            child_cnode,
            mapped_region,
            &mut cnode,
            grandkid_main as extern "C" fn(_) -> (),
            params,
            ut,
            ut,
            slots,
            &thread_priority_authority,
            None,
        )?;
    });
    child_process.start()?;

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
