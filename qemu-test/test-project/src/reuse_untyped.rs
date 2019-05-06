use super::TopLevelError;
use ferros::alloc::micro_alloc::Allocator;
use ferros::alloc::{smart_alloc, ut_buddy};
use ferros::userland::{root_cnode, BootInfo, LocalCNodeSlots, SeL4Error};
use selfe_sys::seL4_BootInfo;
use typenum::*;

pub fn run(raw_boot_info: &'static seL4_BootInfo) -> Result<(), TopLevelError> {
    let BootInfo {
        root_page_directory,
        asid_control,
        user_image,
        root_tcb,
        ..
    } = BootInfo::wrap(&raw_boot_info);
    let mut allocator = Allocator::bootstrap(&raw_boot_info)?;
    let (root_cnode, local_slots) = root_cnode(&raw_boot_info);
    let prime_ut = allocator
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

    unsafe {
        let (r, second_ut) =
            prime_ut.with_temporary(&root_cnode, move |inner_ut| -> Result<(), SeL4Error> {
                let (a, b) = inner_ut.split(slot_a)?;
                track_ref.set(track_ref.get() + 1);
                Ok(())
            })?;
        r?;

        let (r, third_ut) =
            second_ut.with_temporary(&root_cnode, move |inner_ut| -> Result<(), SeL4Error> {
                let (a, b) = inner_ut.split(slot_b)?;
                track_ref.set(track_ref.get() + 1);
                Ok(())
            })?;
        r?;

        let (r, _fourth_ut) =
            third_ut.with_temporary(&root_cnode, |inner_ut| -> Result<(), SeL4Error> {
                let (a, b) = inner_ut.split(slot_c)?;
                track_ref.set(track_ref.get() + 1);

                // Demonstrate nested use (left side)
                let (a_res, a) =
                    a.with_temporary(&root_cnode, |inner_left| -> Result<(), SeL4Error> {
                        let (c, d) = inner_left.split(slot_d)?;
                        track_ref.set(track_ref.get() + 1);
                        Ok(())
                    })?;
                a_res?;

                // Demonstrate nested re-use (left side)
                let (a_res, _a) =
                    a.with_temporary(&root_cnode, |inner_left| -> Result<(), SeL4Error> {
                        let (c, d) = inner_left.split(slot_e)?;
                        track_ref.set(track_ref.get() + 1);
                        Ok(())
                    })?;
                a_res?;

                // Demonstrate nested use (right side)
                let (b_res, b) =
                    b.with_temporary(&root_cnode, |inner_right| -> Result<(), SeL4Error> {
                        let (d, e) = inner_right.split(slot_f)?;
                        track_ref.set(track_ref.get() + 1);
                        Ok(())
                    })?;
                b_res?;
                // Demonstrate nested re-use (right side)
                let (b_res, b) =
                    b.with_temporary(&root_cnode, move |inner_right| -> Result<(), SeL4Error> {
                        let (d, e) = inner_right.split(slot_g)?;
                        track_ref.set(track_ref.get() + 1);
                        Ok(())
                    })?;
                b_res?;

                Ok(())
            })?;
        r?;
    }
    assert_eq!(7, track.get());
    debug_println!("\nSuccessfully reused untyped multiple times\n");

    Ok(())
}
