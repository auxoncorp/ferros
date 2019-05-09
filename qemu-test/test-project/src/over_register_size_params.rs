use super::TopLevelError;

use ferros::alloc::{self, micro_alloc, smart_alloc};
use typenum::*;

use ferros::userland::{retype, retype_cnode, root_cnode, BootInfo, RetypeForSetup, VSpace, VSpaceScratchSlice};
use selfe_sys::*;

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

    smart_alloc!(|slots from local_slots, ut from uts| {
        let (mut local_vspace_scratch, root_page_directory) = VSpaceScratchSlice::from_parts(
            slots, ut, root_page_directory)?;

        let (asid_pool, _asid_control) = asid_control.allocate_asid_pool(ut, slots)?;
        let (child_asid, asid_pool) = asid_pool.alloc();
        let child_vspace = VSpace::new(ut, slots, child_asid, &user_image, &root_cnode)?;

        let (child_cnode, _child_slots) = retype_cnode::<U12>(ut, slots)?;

        let params = {
            let mut nums = [0xaaaaaaaa; 140];
            nums[0] = 0xbbbbbbbb;
            nums[139] = 0xcccccccc;
            OverRegisterSizeParams { nums }
        };

        let (child_process, _) = child_vspace.prepare_thread(
            proc_main,
            params,
            ut,
            slots,
            &mut local_vspace_scratch,
        )?;
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

pub struct OverRegisterSizeParams {
    pub nums: [usize; 140],
}

impl RetypeForSetup for OverRegisterSizeParams {
    type Output = OverRegisterSizeParams;
}

pub extern "C" fn proc_main(params: OverRegisterSizeParams) {
    debug_println!(
        "The child process saw a first value of {:08x}, a mid value of {:08x}, and a last value of {:08x}",
        params.nums[0],
        params.nums[70],
        params.nums[139]
    );
}
