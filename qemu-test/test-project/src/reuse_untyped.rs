use typenum::*;

use ferros::cap::{LocalCNode, LocalCNodeSlots, LocalCap, Untyped};
use ferros::error::SeL4Error;
use ferros_test::ferros_test;

use super::TopLevelError;

#[ferros_test]
pub fn reuse_untyped(
    mut prime_ut: LocalCap<Untyped<U27>>,
    root_cnode: &LocalCap<LocalCNode>,
    slot_a: LocalCNodeSlots<U2>,
    slot_b: LocalCNodeSlots<U2>,
    slot_c: LocalCNodeSlots<U2>,
    slot_d: LocalCNodeSlots<U2>,
    slot_e: LocalCNodeSlots<U2>,
    slot_f: LocalCNodeSlots<U2>,
    slot_g: LocalCNodeSlots<U2>,
) -> Result<(), TopLevelError> {
    let track = core::cell::Cell::new(0);
    let track_ref = &track;
    // TODO - check correct reuse following inner error

    prime_ut.with_temporary(&root_cnode, move |inner_ut| -> Result<(), SeL4Error> {
        let (_a, _b) = inner_ut.split(slot_a)?;
        track_ref.set(track_ref.get() + 1);
        Ok(())
    })??;

    prime_ut.with_temporary(&root_cnode, move |inner_ut| -> Result<(), SeL4Error> {
        let (_a, _b) = inner_ut.split(slot_b)?;
        track_ref.set(track_ref.get() + 1);
        Ok(())
    })??;

    prime_ut.with_temporary(&root_cnode, |inner_ut| -> Result<(), SeL4Error> {
        let (mut a, mut b) = inner_ut.split(slot_c)?;
        track_ref.set(track_ref.get() + 1);

        // Demonstrate nested use (left side)
        a.with_temporary(&root_cnode, |inner_left| -> Result<(), SeL4Error> {
            let (_c, _d) = inner_left.split(slot_d)?;
            track_ref.set(track_ref.get() + 1);
            Ok(())
        })??;

        // Demonstrate nested re-use (left side)
        a.with_temporary(&root_cnode, |inner_left| -> Result<(), SeL4Error> {
            let (_c, _d) = inner_left.split(slot_e)?;
            track_ref.set(track_ref.get() + 1);
            Ok(())
        })??;

        // Demonstrate nested use (right side)
        b.with_temporary(&root_cnode, |inner_right| -> Result<(), SeL4Error> {
            let (_e, _f) = inner_right.split(slot_f)?;
            track_ref.set(track_ref.get() + 1);
            Ok(())
        })??;
        // Demonstrate nested re-use (right side)
        b.with_temporary(&root_cnode, move |inner_right| -> Result<(), SeL4Error> {
            let (_e, _f) = inner_right.split(slot_g)?;
            track_ref.set(track_ref.get() + 1);
            Ok(())
        })??;

        Ok(())
    })??;
    assert_eq!(7, track.get());
    Ok(())
}
