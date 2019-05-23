use super::TopLevelError;
use ferros::alloc::micro_alloc::Allocator;
use ferros::userland::{root_cnode, LocalCNodeSlots, SeL4Error};
use selfe_sys::seL4_BootInfo;
use typenum::*;

pub fn run(raw_boot_info: &'static seL4_BootInfo) -> Result<(), TopLevelError> {
    let mut allocator = Allocator::bootstrap(&raw_boot_info)?;
    let (_root_cnode, local_slots) = root_cnode(&raw_boot_info);
    let ut_20 = allocator.get_untyped::<U20>().expect("alloc failure a");
    let (slots, local_slots) = local_slots.alloc();
    let (ut_a, ut_b, ut_c, ut_18) = ut_20.quarter(slots)?;
    let (slots, mut local_slots) = local_slots.alloc();
    let (ut_d, ut_e, ut_f, _ut_g) = ut_18.quarter(slots)?;

    let track = core::cell::Cell::new(0);
    let track_ref = &track;

    debug_println!("about to start temporary use tests");

    // TODO - check correct reuse following inner error
    local_slots.with_temporary(move |inner_slots| -> Result<(), SeL4Error> {
        let (slots, _inner_slots) = inner_slots.alloc();
        let (_a, _b) = ut_a.split(slots)?;
        track_ref.set(track_ref.get() + 1);
        Ok(())
    })??;
    debug_println!("finished first temporary use");

    local_slots.with_temporary(move |inner_slots| -> Result<(), SeL4Error> {
        let (slots, _inner_slots) = inner_slots.alloc();
        let (_a, _b) = ut_b.split(slots)?; // Expect it to blow up here
        track_ref.set(track_ref.get() + 1);
        Ok(())
    })??;
    debug_println!("finished second temporary use");

    local_slots.with_temporary(move |inner_slots| -> Result<(), SeL4Error> {
        let (mut slots_a, mut slots_b): (LocalCNodeSlots<U4>, _) = inner_slots.alloc();
        // Nested use (left)
        slots_a.with_temporary(move |inner_slots_a| -> Result<(), SeL4Error> {
            let (slots, _) = inner_slots_a.alloc();
            let (_a, _b) = ut_c.split(slots)?;
            track_ref.set(track_ref.get() + 1);
            Ok(())
        })??;
        debug_println!("finished nested use left");

        // Nested reuse (left)
        slots_a.with_temporary(move |inner_slots_a| -> Result<(), SeL4Error> {
            let (slots, _) = inner_slots_a.alloc();
            let (_a, _b) = ut_d.split(slots)?;
            track_ref.set(track_ref.get() + 1);
            Ok(())
        })??;
        debug_println!("finished nested reuse left");

        // Nested use (right)
        slots_b.with_temporary(move |inner_slots_b| -> Result<(), SeL4Error> {
            let (slots, _) = inner_slots_b.alloc();
            let (_a, _b) = ut_e.split(slots)?;
            track_ref.set(track_ref.get() + 1);
            Ok(())
        })??;
        debug_println!("finished nested use right");

        // Nested reuse (right)
        slots_b.with_temporary(move |inner_slots_b| -> Result<(), SeL4Error> {
            let (slots, _) = inner_slots_b.alloc();
            let (_a, _b) = ut_f.split(slots)?;
            track_ref.set(track_ref.get() + 1);
            Ok(())
        })??;
        debug_println!("finished nested reuse right");

        track_ref.set(track_ref.get() + 1);
        Ok(())
    })??;

    debug_println!("finished reuse with inner reuse");
    assert_eq!(7, track.get());
    debug_println!("\nSuccessfully reused slots multiple times\n");

    Ok(())
}
