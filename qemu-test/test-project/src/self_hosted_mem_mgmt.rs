use super::TopLevelError;

use ferros::alloc::{smart_alloc, ut_buddy};
use typenum::*;

use ferros::bootstrap::UserImage;
use ferros::cap::*;
use ferros::userland::{
    fault_or_message_channel, CapRights, FaultOrMessage, RetypeForSetup, SelfHostedProcess, Sender,
};
use ferros::vspace::*;

#[ferros_test::ferros_test]
pub fn self_hosted_mem_mgmt(
    local_slots: LocalCNodeSlots<U32768>,
    local_ut: LocalCap<Untyped<U20>>,
    asid_pool: LocalCap<ASIDPool<U1>>,
    local_mapped_region: MappedMemoryRegion<U17, shared_status::Exclusive>,
    root_cnode: &LocalCap<LocalCNode>,
    user_image: &UserImage<role::Local>,
    tpa: &LocalCap<ThreadPriorityAuthority>,
) -> Result<(), TopLevelError> {
    debug_println!("running self hosted mem management");
    let uts = ut_buddy(local_ut);

    smart_alloc!(|slots: local_slots, ut: uts| {
        let (child_cnode, child_slots) = retype_cnode::<U12>(ut, slots)?;
        debug_println!("have child_cnode");

        let ut12: LocalCap<Untyped<U12>> = ut;

        smart_alloc! {|slots_c: child_slots| {
            let cap_transfer_slots = slots_c;
            let (cnode_for_child, slots_for_child) =
                child_cnode.generate_self_reference(&root_cnode, slots_c)?;
            debug_println!("self ref generated");
            let child_ut12 = ut12.move_to_slot(&root_cnode, slots_c)?;
            debug_println!("have ut12");
            let (fault_source, outcome_sender, handler) = fault_or_message_channel(
                &root_cnode,
                ut,
                slots,
                slots_c,
                slots,
            )?;
            debug_println!("have fault");
        }}

        let (child_paging_slots, slots_for_child) = slots_for_child.alloc();

        let params = ProcParams {
            value: 42,
            child_slots: slots_for_child,
            untyped: child_ut12,
            outcome_sender,
        };

        let (child_asid, _asid_pool) = asid_pool.alloc();

        let child_root = retype(ut, slots)?;
        let child_vspace_slots: LocalCNodeSlots<U1024> = slots;
        let child_vspace_ut: LocalCap<Untyped<U14>> = ut;

        let child_vspace = VSpace::new(
            child_root,
            child_asid,
            child_vspace_slots.weaken(),
            child_vspace_ut.weaken(),
            ProcessCodeImageConfig::ReadOnly,
            user_image,
            root_cnode,
        )?;

        debug_println!("making process");
        let sh_process = SelfHostedProcess::new(
            child_vspace,
            child_cnode,
            local_mapped_region,
            root_cnode,
            sh_main,
            params,
            ut,
            ut,
            slots,
            cap_transfer_slots,
            child_paging_slots,
            tpa,
            None, // fault
        )?;
    });

    debug_println!("starting process");
    sh_process.start()?;
    debug_println!("process started");

    match handler.await_message()? {
        FaultOrMessage::Message(true) => Ok(()),
        _ => Err(TopLevelError::TestAssertionFailure(
            "Child process should have reported success",
        )),
    }
}

pub struct ProcParams<Role: CNodeRole> {
    pub value: usize,
    pub child_slots: Cap<CNodeSlotsData<U1, Role>, Role>,
    pub untyped: Cap<Untyped<U12>, Role>,
    pub outcome_sender: Sender<bool, Role>,
}

impl RetypeForSetup for ProcParams<role::Local> {
    type Output = ProcParams<role::Child>;
}

pub extern "C" fn sh_main(mut vspace: VSpace, params: ProcParams<role::Local>) {
    debug_println!("in sh_main");
    let ProcParams {
        value,
        child_slots,
        untyped,
        outcome_sender,
    } = params;
    debug_println!("have params");
    let unmapped_region =
        UnmappedMemoryRegion::new(untyped, child_slots).expect("retyping memory failed");
    let mapped_region = vspace
        .map_region(unmapped_region, CapRights::RW)
        .expect("mapping region failed");
    let vaddr = mapped_region.vaddr() as *mut u8;
    let val_at_ptr = unsafe {
        debug_println!("derefing the addr sigh");
        *vaddr = 8;
        *vaddr
    };
    outcome_sender
        .blocking_send(&(params.value == 42 && val_at_ptr == 8))
        .expect("Found value does not match expectations")
}
