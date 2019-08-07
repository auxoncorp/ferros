use super::TopLevelError;

use typenum::*;

use ferros::alloc::ut_buddy::weak_ut_buddy;
use ferros::cap::*;

#[ferros_test::ferros_test]
pub fn wutbuddy(
    local_slots: LocalCNodeSlots<U64>,
    local_ut: LocalCap<Untyped<U13>>,
) -> Result<(), TopLevelError> {
    let mut wut = weak_ut_buddy(local_ut.weaken());
    let (strong_slot, local_slots) = local_slots.alloc();
    let mut weak_slots = local_slots.weaken();
    let weak_12 = wut.alloc(&mut weak_slots, 12)?;

    // Did we get a thing of the right size?
    assert_eq!(weak_12.size_bits(), 12);

    // Can we actually use it as that size?
    let _ = weak_12.retype::<Page<page_state::Unmapped>>(&mut weak_slots)?;

    let ut12 = wut.alloc_strong::<U12>(&mut weak_slots)?;

    // Same story with the strong untyped.
    let _ = ut12.retype::<Page<page_state::Unmapped>, role::Local>(strong_slot)?;
    Ok(())
}
