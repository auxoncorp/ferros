use super::TopLevelError;
use selfe_sys::seL4_BootInfo;

use ferros::alloc::{self, micro_alloc, smart_alloc};
use typenum::*;

use ferros::userland::{retype, retype_cnode, root_cnode, BootInfo, RetypeForSetup, VSpace};

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
            .get_untyped::<U27>()
            .expect("initial alloc failure"),
    );

    smart_alloc!(|slots from local_slots, ut from uts| {
        let unmapped_scratch_page_table = retype(ut, slots)?;
        let (mut scratch_page_table, mut root_page_directory) =
            root_page_directory.map_page_table(unmapped_scratch_page_table)?;

        let (child_cnode, child_slots) = retype_cnode::<U12>(ut, slots)?;
        let params = ProcParams { value: 42 };

        let (asid_pool, _asid_control) = asid_control.allocate_asid_pool(ut, slots)?;
        let (child_asid, asid_pool) = asid_pool.alloc();
        let child_vspace = VSpace::new(ut, slots, child_asid, &user_image, &root_cnode,
                                       &mut root_page_directory)?;

        let (child_process, _) = child_vspace.prepare_thread(
            proc_main,
            params,
            ut,
            slots,
            &mut scratch_page_table,
            &mut root_page_directory,
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

pub extern "C" fn proc_main(params: ProcParams) {
    debug_println!("\nThe value inside the process is {}\n", params.value);
}
