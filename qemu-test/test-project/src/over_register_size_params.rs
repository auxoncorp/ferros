use super::TopLevelError;

use ferros::alloc::{self, micro_alloc, smart_alloc};
use typenum::*;

use ferros::userland::{retype, retype_cnode, root_cnode, BootInfo, RetypeForSetup, VSpace};
use selfe_sys::*;

pub fn run(raw_boot_info: &'static seL4_BootInfo) -> Result<(), TopLevelError> {
    let mut allocator = micro_alloc::Allocator::bootstrap(&raw_boot_info)?;
    let (root_cnode, local_slots) = root_cnode(&raw_boot_info);

    let ut27 = allocator
        .get_untyped::<U27>()
        .expect("initial alloc failure");
    let uts = alloc::ut_buddy(ut27);

    smart_alloc!(|slots from local_slots, ut from uts| {
        let boot_info = BootInfo::wrap(raw_boot_info, ut, slots);

        let unmapped_scratch_page_table = retype(ut, slots)?;
        let (mut scratch_page_table, boot_info) =
            boot_info.map_page_table(unmapped_scratch_page_table)?;

        let (child_cnode, _child_slots) = retype_cnode::<U12>(ut, slots)?;
    });

    let params = {
        let mut nums = [0xaaaaaaaa; 140];
        nums[0] = 0xbbbbbbbb;
        nums[139] = 0xcccccccc;
        OverRegisterSizeParams { nums }
    };

    smart_alloc!(|slots from local_slots, ut from uts| {
        let (child_vspace, mut boot_info) = VSpace::new(boot_info, ut, &root_cnode, slots)?;

        let (child_process, _) = child_vspace.prepare_thread(
            proc_main,
            params,
            ut,
            slots,
            &mut scratch_page_table,
            &mut boot_info.page_directory,
        )?;
    });

    child_process.start(child_cnode, None, &boot_info.tcb, 255)?;

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
