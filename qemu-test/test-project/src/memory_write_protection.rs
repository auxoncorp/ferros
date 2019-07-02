use ferros::alloc::{smart_alloc, ut_buddy};
use typenum::*;

use ferros::arch::fault::Fault;
use ferros::bootstrap::UserImage;
use ferros::cap::*;
use ferros::userland::{FaultSinkSetup, ReadyProcess, RetypeForSetup};
use ferros::vspace::*;

use super::TopLevelError;

type U33768 = Sum<U32768, U1000>;

#[ferros_test::ferros_test]
pub fn memory_write_protection<'a, 'b, 'c>(
    local_slots: LocalCNodeSlots<U33768>,
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
        let child_vspace_slots: LocalCNodeSlots<U4096> = slots;
        let child_vspace_ut: LocalCap<Untyped<U12>> = ut;
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
        let params = ProcParams {};

        let setup = FaultSinkSetup::new(&root_cnode, ut, slots, slots)?;
        let (child_slot_for_fault_source, _child_slots) = child_slots.alloc();
        let fault_source =
            setup.add_fault_source(&root_cnode, child_slot_for_fault_source, Badge::from(0))?;
        let sink = setup.sink();

        let child_process = ReadyProcess::new(
            &mut child_vspace,
            child_cnode,
            local_mapped_region,
            root_cnode,
            proc_main,
            params,
            ut,
            ut,
            slots,
            tpa,
            Some(fault_source),
        )?;
    });
    child_process.start()?;

    match sink.wait_for_fault() {
        Fault::VMFault(_) => Ok(()),
        o => Err(TopLevelError::TestAssertionFailure(
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
