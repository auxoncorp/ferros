use super::TopLevelError;
use ferros::alloc::micro_alloc::Allocator;
use ferros::userland::{root_cnode, LocalCNodeSlots, SeL4Error};
use selfe_sys::seL4_BootInfo;
use typenum::*;

pub fn run(raw_boot_info: &'static seL4_BootInfo) -> Result<(), TopLevelError> {
    let mut allocator = Allocator::bootstrap(&raw_boot_info)?;
    let (root_cnode, local_slots) = root_cnode(&raw_boot_info);
    let mut prime_ut = allocator
        .get_untyped::<U27>()
        .expect("initial alloc failure");
    let (slot_a, local_slots): (LocalCNodeSlots<U2>, _) = local_slots.alloc();
    let (slot_b, local_slots): (LocalCNodeSlots<U2>, _) = local_slots.alloc();
    let (slot_c, local_slots): (LocalCNodeSlots<U2>, _) = local_slots.alloc();
    let (slot_d, local_slots): (LocalCNodeSlots<U2>, _) = local_slots.alloc();
    let (slot_e, local_slots): (LocalCNodeSlots<U2>, _) = local_slots.alloc();
    let (slot_f, local_slots): (LocalCNodeSlots<U2>, _) = local_slots.alloc();
    let (slot_g, local_slots): (LocalCNodeSlots<U2>, _) = local_slots.alloc();

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
    debug_println!("\nSuccessfully reused untyped multiple times\n");

    Ok(())
}
