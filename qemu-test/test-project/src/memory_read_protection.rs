use typenum::*;

use ferros::alloc::{smart_alloc, ut_buddy};
use ferros::arch::fault::Fault;
use ferros::bootstrap::UserImage;
use ferros::cap::{
    retype_cnode, role, ASIDPool, Badge, LocalCNode, LocalCNodeSlots, LocalCap,
    ThreadPriorityAuthority, Untyped,
};
use ferros::userland::{FaultSinkSetup, RetypeForSetup};
use ferros::vspace::{VSpace, VSpaceScratchSlice};

use super::TopLevelError;

#[ferros_test::ferros_test]
pub fn memory_read_protection(
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
        let x: *const usize = 0x88888888usize as _;
        debug_println!("Value from arbitrary memory is: {}", *x);
    }

    debug_println!("This is after the segfaulting code, and should not be printed.");
}
