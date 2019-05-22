use core::marker::PhantomData;

use selfe_sys::*;
use typenum::*;

use crate::arch;
use crate::cap::*;
use crate::error::SeL4Error;

mod resources;
mod types;

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

#[derive(Debug, Clone, Copy)]
pub enum TestOutcome {
    Success,
    Failure,
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
        scratch,
        cnode,
        thread_authority,
        user_image,
    } = resources;
    let mut successes = 0;
    let mut failures = 0;
    for t in tests {
        with_temporary_resources(
            slots,
            untyped,
            asid_pool,
            |s, u, a| -> Result<(), SeL4Error> {
                let (name, outcome) = t(s, u, a, scratch, cnode, thread_authority, user_image);
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
pub fn with_temporary_resources<SlotCount: Unsigned, BitSize: Unsigned, E, F>(
    slots: &mut LocalCNodeSlots<SlotCount>,
    untyped: &mut LocalCap<Untyped<BitSize>>,
    asid_pool: &mut LocalCap<ASIDPool<arch::ASIDPoolSize>>,
    f: F,
) -> Result<Result<(), E>, SeL4Error>
where
    F: FnOnce(
        LocalCNodeSlots<SlotCount>,
        LocalCap<Untyped<BitSize>>,
        LocalCap<ASIDPool<arch::ASIDPoolSize>>,
    ) -> Result<(), E>,
{
    // Call the function with an alias/copy of self
    let r = f(
        Cap::internal_new(slots.cptr, slots.cap_data.offset),
        Cap {
            cptr: untyped.cptr,
            cap_data: Untyped {
                _bit_size: PhantomData,
                _kind: PhantomData,
            },
            _role: PhantomData,
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
    );
    unsafe { slots.revoke_in_reverse() }

    // Clean up any child/derived capabilities that may have been created from the memory
    // Because the slots and the untyped are both Local, the slots' parent CNode capability pointer
    // must be the same as the untyped's parent CNode
    let err = unsafe {
        seL4_CNode_Revoke(
            slots.cptr,          // _service
            untyped.cptr,        // index
            seL4_WordBits as u8, // depth
        )
    };
    if err != 0 {
        return Err(SeL4Error::CNodeRevoke(err));
    }
    Ok(r)
}
