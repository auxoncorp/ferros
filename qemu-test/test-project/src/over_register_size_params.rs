use typenum::*;

use ferros::alloc::{smart_alloc, ut_buddy};
use ferros::bootstrap::UserImage;
use ferros::cap::*;
use ferros::userland::{fault_or_message_channel, FaultOrMessage, RetypeForSetup, Sender};
use ferros::vspace::{VSpace, VSpaceScratchSlice};

use super::TopLevelError;

/// Test that we can pass process parameters with content larger than that will
/// fit in the TCB registers
#[ferros_test::ferros_test]
pub fn over_register_size_params(
    local_slots: LocalCNodeSlots<U32768>,
    local_ut: LocalCap<Untyped<U20>>,
    asid_pool: LocalCap<ASIDPool<U1>>,
    local_vspace_scratch: &mut VSpaceScratchSlice<role::Local>,
    root_cnode: &LocalCap<LocalCNode>,
    user_image: &UserImage<role::Local>,
    tpa: &LocalCap<ThreadPriorityAuthority>,
) -> Result<(), TopLevelError> {
    let uts = ut_buddy(local_ut);

    smart_alloc!(|slots: local_slots, ut: uts| {
        let (child_asid, _asid_pool) = asid_pool.alloc();
        let child_vspace = VSpace::new(ut, slots, child_asid, &user_image, &root_cnode)?;

        let (child_cnode, child_slots) = retype_cnode::<U12>(ut, slots)?;
        let (child_fault_source_slot, _child_slots) = child_slots.alloc();
        let (fault_source, outcome_sender, handler) =
            fault_or_message_channel(&root_cnode, ut, slots, child_fault_source_slot, slots)?;

        let params = {
            let mut nums = [0xaaaaaaaa; 140];
            nums[0] = 0xbbbbbbbb;
            nums[139] = 0xcccccccc;
            OverRegisterSizeParams {
                nums,
                outcome_sender,
            }
        };

        let (child_process, _) =
            child_vspace.prepare_thread(proc_main, params, ut, slots, local_vspace_scratch)?;
    });

    child_process.start(child_cnode, Some(fault_source), tpa, 255)?;

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
    pub nums: [usize; 140],
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
        .blocking_send(
            &(nums[0] == 0xbbbbbbbb && nums[70] == 0xaaaaaaaa && nums[139] == 0xcccccccc),
        )
        .expect("Failure sending test assertion outcome");
}
