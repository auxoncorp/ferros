use super::TopLevelError;
use selfe_sys::seL4_BootInfo;

use ferros::alloc::{self, micro_alloc, smart_alloc};
use typenum::*;

use ferros::userland::{
    retype_cnode, root_cnode, BootInfo, RetypeForSetup, VSpace, VSpaceScratchSlice,
};

pub fn run(raw_boot_info: &'static seL4_BootInfo) -> Result<(), TopLevelError> {
    let BootInfo {
        root_page_directory,
        asid_control,
        user_image,
        root_tcb,
        ..
    } = BootInfo::wrap(&raw_boot_info);
    let mut allocator = micro_alloc::Allocator::bootstrap(&raw_boot_info)?;
    let (root_cnode, local_slots) = root_cnode(&raw_boot_info);
    let uts = alloc::ut_buddy(
        allocator
            .get_untyped::<U20>()
            .expect("initial alloc failure"),
    );

    smart_alloc!(|slots: local_slots, ut: uts| {
        let (mut local_vspace_scratch, root_page_directory) =
            VSpaceScratchSlice::from_parts(slots, ut, root_page_directory)?;

        let (asid_pool, _asid_control) = asid_control.allocate_asid_pool(ut, slots)?;
        let (child_asid, asid_pool) = asid_pool.alloc();
        let child_vspace = VSpace::new(ut, slots, child_asid, &user_image, &root_cnode)?;

        let (child_cnode, child_slots) = retype_cnode::<U12>(ut, slots)?;
        let params = ProcParams { value: 42 };

        let (child_process, _) =
            child_vspace.prepare_thread(proc_main, params, ut, slots, &mut local_vspace_scratch)?;
    });

    child_process.start(child_cnode, None, root_tcb.as_ref(), 255)?;

    Ok(())
}

pub struct ProcParams {
    pub value: usize,
}

impl RetypeForSetup for ProcParams {
    type Output = ProcParams;
}

pub extern "C" fn proc_main(_params: ProcParams) {
    debug_println!("\nAttempting to cause a segmentation fault...\n");

    unsafe {
        let x: *const usize = 0x88888888usize as _;
        debug_println!("Value from arbitrary memory is: {}", *x);
    }

    debug_println!("This is after the segfaulting code, and should not be printed.");
}
