use typenum::*;

use ferros::alloc::{smart_alloc, ut_buddy};
use ferros::bootstrap::UserImage;
use ferros::cap::*;
use ferros::userland::{
    fault_or_message_channel, FaultOrMessage, RetypeForSetup, Sender, StandardProcess,
};
use ferros::vspace::*;

use super::TopLevelError;

/// Test that we can pass process parameters with content larger than that will
/// fit in the TCB registers
#[ferros_test::ferros_test]
pub fn over_register_size_params<'a, 'b, 'c>(
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
        let child_vspace_slots: LocalCNodeSlots<U1024> = slots;
        let child_vspace_ut: LocalCap<Untyped<U15>> = ut;
        let mut child_vspace = VSpace::new(
            retype(ut, slots)?,
            child_asid,
            child_vspace_slots.weaken(),
            child_vspace_ut.weaken(),
            ProcessCodeImageConfig::ReadOnly,
            user_image,
            root_cnode,
        )?;

        let (child_cnode, child_slots) = retype_cnode::<U12>(ut, slots)?;
        let (child_fault_source_slot, _child_slots) = child_slots.alloc();
        let (fault_source, outcome_sender, handler) =
            fault_or_message_channel(&root_cnode, ut, slots, child_fault_source_slot, slots)?;

        let params = {
            let nums = [427; 10];
            OverRegisterSizeParams {
                nums,
                outcome_sender,
            }
        };
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
            None,
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

pub struct ProcParams {
    pub value: usize,
}

impl RetypeForSetup for ProcParams {
    type Output = ProcParams;
}

pub struct OverRegisterSizeParams<Role: CNodeRole> {
    pub nums: [usize; 10],
    pub outcome_sender: Sender<bool, Role>,
}

impl RetypeForSetup for OverRegisterSizeParams<role::Local> {
    type Output = OverRegisterSizeParams<role::Child>;
}

pub extern "C" fn proc_main(params: OverRegisterSizeParams<role::Local>) {
    let OverRegisterSizeParams {
        nums,
        outcome_sender,
    } = params;
    outcome_sender
        .blocking_send(&(nums[7] == 427))
        .expect("Failure sending test assertion outcome");
}
