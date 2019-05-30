use typenum::*;

use ferros::cap::{LocalCNodeSlots, LocalCap, Untyped};
use ferros::error::SeL4Error;

use super::TopLevelError;

#[ferros_test::ferros_test]
pub fn reuse_slots(
    local_slots: LocalCNodeSlots<U100>,
    ut_20: LocalCap<Untyped<U20>>,
) -> Result<(), TopLevelError> {
    let (slots, local_slots) = local_slots.alloc();
    let (ut_a, ut_b, ut_c, ut_18) = ut_20.quarter(slots)?;
    let (slots, mut local_slots) = local_slots.alloc();
    let (ut_d, ut_e, ut_f, _ut_g) = ut_18.quarter(slots)?;

    let track = core::cell::Cell::new(0);
    let track_ref = &track;

    // TODO - check correct reuse following inner error
    local_slots.with_temporary(move |inner_slots| -> Result<(), SeL4Error> {
        let (slots, _inner_slots) = inner_slots.alloc();
        let (_a, _b) = ut_a.split(slots)?;
        track_ref.set(track_ref.get() + 1);
        Ok(())
    })??;

    local_slots.with_temporary(move |inner_slots| -> Result<(), SeL4Error> {
        let (slots, _inner_slots) = inner_slots.alloc();
        let (_a, _b) = ut_b.split(slots)?; // Expect it to blow up here
        track_ref.set(track_ref.get() + 1);
        Ok(())
    })??;

    local_slots.with_temporary(move |inner_slots| -> Result<(), SeL4Error> {
        let (mut slots_a, mut slots_b): (LocalCNodeSlots<U4>, _) = inner_slots.alloc();
        // Nested use (left)
        slots_a.with_temporary(move |inner_slots_a| -> Result<(), SeL4Error> {
            let (slots, _) = inner_slots_a.alloc();
            let (_a, _b) = ut_c.split(slots)?;
            track_ref.set(track_ref.get() + 1);
            Ok(())
        })??;

        // Nested reuse (left)
        slots_a.with_temporary(move |inner_slots_a| -> Result<(), SeL4Error> {
            let (slots, _) = inner_slots_a.alloc();
            let (_a, _b) = ut_d.split(slots)?;
            track_ref.set(track_ref.get() + 1);
            Ok(())
        })??;

        // Nested use (right)
        slots_b.with_temporary(move |inner_slots_b| -> Result<(), SeL4Error> {
            let (slots, _) = inner_slots_b.alloc();
            let (_a, _b) = ut_e.split(slots)?;
            track_ref.set(track_ref.get() + 1);
            Ok(())
        })??;

        // Nested reuse (right)
        slots_b.with_temporary(move |inner_slots_b| -> Result<(), SeL4Error> {
            let (slots, _) = inner_slots_b.alloc();
            let (_a, _b) = ut_f.split(slots)?;
            track_ref.set(track_ref.get() + 1);
            Ok(())
        })??;

        track_ref.set(track_ref.get() + 1);
        Ok(())
    })??;
    if 7 == track.get() {
        Ok(())
    } else {
        Err(TopLevelError::TestAssertionFailure(
            "Unexpected number of tracked reuses",
        ))
    }
}
