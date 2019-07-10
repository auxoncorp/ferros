use core::marker::PhantomData;
use core::ops::Sub;

use selfe_sys::*;
use typenum::*;

use crate::arch::{self, PageBits};
use crate::cap::*;
use crate::error::{ErrorExt, SeL4Error};
use crate::pow::{Pow, _Pow};

mod resources;
mod types;

use crate::vspace::MappedMemoryRegion;
pub use resources::*;
pub use types::*;

impl TestReporter for crate::debug::DebugOutHandle {
    fn report(&mut self, test_name: &'static str, outcome: TestOutcome) {
        use core::fmt::Write;
        let _ = writeln!(
            self,
            "test {} ... {}",
            test_name,
            if outcome == TestOutcome::Success {
                "ok"
            } else {
                "FAILED"
            }
        );
    }

    fn summary(&mut self, passed: u32, failed: u32) {
        use core::fmt::Write;
        let _ = writeln!(
            self,
            "\ntest result: {}. {} passed; {} failed;",
            if failed == 0 { "ok" } else { "FAILED" },
            passed,
            failed
        );
    }
}

// TODO - a TestReporter impl for a UART

/// Execute multiple tests, reporting their results
/// in a streaming fashion followed by a final summary.
///
/// The &RunTest instances are expected to be references
/// to functions annotated with `#[ferros_test]`, which
/// transforms said tests to conform with the RunTest signature
pub fn execute_tests<'t, R: types::TestReporter>(
    mut reporter: R,
    resources: resources::TestResourceRefs<'t>,
    tests: &[&types::RunTest],
) -> Result<types::TestOutcome, SeL4Error> {
    let resources::TestResourceRefs {
        slots,
        untyped,
        asid_pool,
        mut scratch,
        mapped_memory_region,
        cnode,
        thread_authority,
        user_image,
        irq_control,
    } = resources;
    let mut successes = 0;
    let mut failures = 0;
    for t in tests {
        with_temporary_resources(
            slots,
            untyped,
            asid_pool,
            mapped_memory_region,
            irq_control,
            |inner_slots,
             inner_untyped,
             inner_asid_pool,
             inner_mapped_memory_region,
             inner_irq_control|
             -> Result<(), SeL4Error> {
                let (name, outcome) = t(
                    inner_slots,
                    inner_untyped,
                    inner_asid_pool,
                    &mut scratch,
                    inner_mapped_memory_region,
                    cnode,
                    thread_authority,
                    user_image,
                    inner_irq_control,
                );
                reporter.report(name, outcome);
                if outcome == types::TestOutcome::Success {
                    successes += 1;
                } else {
                    failures += 1;
                }
                Ok(())
            },
        )??;
    }
    reporter.summary(successes, failures);
    Ok(if failures == 0 {
        types::TestOutcome::Success
    } else {
        types::TestOutcome::Failure
    })
}

/// Gain temporary access to some slots and memory for use in a function context.
/// When the passed function call is complete, all capabilities
/// in this range will be revoked and deleted and the memory reclaimed.
pub fn with_temporary_resources<
    SlotCount: Unsigned,
    UntypedBitSize: Unsigned,
    MappedBitSize: Unsigned,
    ASIDPoolSlots: Unsigned,
    InnerError,
    Func,
>(
    slots: &mut LocalCNodeSlots<SlotCount>,
    untyped: &mut LocalCap<Untyped<UntypedBitSize>>,
    asid_pool: &mut LocalCap<ASIDPool<ASIDPoolSlots>>,
    mapped_memory_region: &mut MappedMemoryRegion<
        MappedBitSize,
        crate::vspace::shared_status::Exclusive,
    >,
    irq_control: &mut LocalCap<IRQControl>,
    f: Func,
) -> Result<Result<(), InnerError>, SeL4Error>
where
    Func: FnOnce(
        LocalCNodeSlots<SlotCount>,
        LocalCap<Untyped<UntypedBitSize>>,
        LocalCap<ASIDPool<arch::ASIDPoolSize>>,
        MappedMemoryRegion<MappedBitSize, crate::vspace::shared_status::Exclusive>,
        LocalCap<IRQControl>,
    ) -> Result<(), InnerError>,

    MappedBitSize: IsGreaterOrEqual<PageBits>,
    MappedBitSize: Sub<PageBits>,
    <MappedBitSize as Sub<PageBits>>::Output: Unsigned,
    <MappedBitSize as Sub<PageBits>>::Output: _Pow,
    Pow<<MappedBitSize as Sub<PageBits>>::Output>: Unsigned,
{
    // Call the function with an alias/copy of the underlying resources
    let r = f(
        Cap::internal_new(slots.cptr, slots.cap_data.offset),
        Cap {
            cptr: untyped.cptr,
            _role: PhantomData,
            cap_data: Untyped {
                _bit_size: PhantomData,
                _kind: PhantomData,
            },
        },
        Cap {
            cptr: asid_pool.cptr,
            _role: PhantomData,
            cap_data: ASIDPool {
                id: asid_pool.cap_data.id,
                next_free_slot: asid_pool.cap_data.next_free_slot,
                _free_slots: PhantomData,
            },
        },
        unsafe { mapped_memory_region.dangerous_internal_alias() },
        Cap {
            cptr: irq_control.cptr,
            _role: PhantomData,
            cap_data: IRQControl {
                available: irq_control.cap_data.available.clone(),
            },
        },
    );
    unsafe { slots.revoke_in_reverse() }

    // Clean up any child/derived capabilities that may have been created from the memory
    // Because the slots and the untyped are both Local, the slots' parent CNode capability pointer
    // must be the same as the untyped's parent CNode
    unsafe {
        seL4_CNode_Revoke(
            slots.cptr,          // _service
            untyped.cptr,        // index
            seL4_WordBits as u8, // depth
        )
    }
    .as_result()
    .map_err(|e| SeL4Error::CNodeRevoke(e))?;
    Ok(r)
}
