use ferros::alloc::{smart_alloc, ut_buddy};
use typenum::*;

use ferros::bootstrap::UserImage;
use ferros::cap::*;
use ferros::userland::{Fault, FaultSinkSetup, RetypeForSetup};
use ferros::vspace::{VSpace, VSpaceScratchSlice};

use super::TopLevelError;

type U33768 = Sum<U32768, U1000>;

#[ferros_test::ferros_test]
pub fn memory_write_protection(
    local_slots: LocalCNodeSlots<U33768>,
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
        let params = ProcParams {};

        let (child_process, _) =
            child_vspace.prepare_thread(proc_main, params, ut, slots, local_vspace_scratch)?;

        let setup = FaultSinkSetup::new(&root_cnode, ut, slots, slots)?;
        let (child_slot_for_fault_source, _child_slots) = child_slots.alloc();
        let fault_source =
            setup.add_fault_source(&root_cnode, child_slot_for_fault_source, Badge::from(0))?;
        let sink = setup.sink();
    });

    child_process.start(child_cnode, Some(fault_source), tpa, 255)?;

    match sink.wait_for_fault() {
        Fault::VMFault(_) => Ok(()),
        _ => Err(TopLevelError::TestAssertionFailure(
            "unexpected fault in memory_read_protection",
        )),
    }
}

pub struct ProcParams {}

impl RetypeForSetup for ProcParams {
    type Output = ProcParams;
}

pub extern "C" fn proc_main(_params: ProcParams) {
    unsafe {
        let x: *mut usize = proc_main as _;
        *x = 42;
    }

    debug_println!("This is after the segfaulting code, and should not be printed.");
}
