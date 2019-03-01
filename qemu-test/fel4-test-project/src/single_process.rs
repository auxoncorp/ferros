use super::TopLevelError;
use ferros::micro_alloc;
use ferros::pow::Pow;
use ferros::userland::{
    role, root_cnode, BootInfo, CNode, CNodeRole, Cap, Endpoint, LocalCap, RetypeForSetup,
    SeL4Error, UnmappedPageTable, Untyped, VSpace,
};
use sel4_sys::*;
use typenum::{Diff, U1, U12, U2, U20, U3, U4096, U6};
type U4095 = Diff<U4096, U1>;

pub fn run(raw_boot_info: &'static seL4_BootInfo) -> Result<(), TopLevelError> {
    #[cfg(test_case = "root_task_runs")]
    {
        debug_println!("\nhello from the root task!\n");
    }

    let mut allocator = micro_alloc::Allocator::bootstrap(&raw_boot_info)?;
    // wrap bootinfo caps
    let root_cnode = root_cnode(&raw_boot_info);

    // find an untyped of size 20 bits (1 meg)
    let ut20 = allocator
        .get_untyped::<U20>()
        .expect("Couldn't find initial untyped");

    let (ut18, ut18b, _, _, root_cnode) = ut20.quarter(root_cnode)?;
    let (child_vspace_ut, child_thread_ut, root_cnode) = ut18b.split(root_cnode)?;
    let (ut16, child_cnode_ut, _, _, root_cnode) = ut18.quarter(root_cnode)?;
    let (ut14, _, _, _, root_cnode) = ut16.quarter(root_cnode)?;
    let (ut12, asid_pool_ut, _, _, root_cnode) = ut14.quarter(root_cnode)?;
    let (ut10, scratch_page_table_ut, _, _, root_cnode) = ut12.quarter(root_cnode)?;
    let (ut8, _, _, _, root_cnode) = ut10.quarter(root_cnode)?;
    let (ut6, _, _, _, root_cnode) = ut8.quarter(root_cnode)?;

    // wrap the rest of the critical boot info
    let (mut boot_info, root_cnode) = BootInfo::wrap(raw_boot_info, asid_pool_ut, root_cnode);

    let (scratch_page_table, root_cnode): (LocalCap<UnmappedPageTable>, _) =
        scratch_page_table_ut.retype_local(root_cnode)?;
    let (mut scratch_page_table, mut boot_info) = boot_info.map_page_table(scratch_page_table)?;

    #[cfg(min_params = "true")]
    let (child_cnode, root_cnode, params) = {
        let (child_cnode, root_cnode) = child_cnode_ut.retype_cnode::<_, U12>(root_cnode)?;

        (child_cnode, root_cnode, ProcParams { value: 42 })
    };

    #[cfg(test_case = "child_process_cap_management")]
    let (child_cnode, root_cnode, params) = {
        let (child_cnode, root_cnode): (LocalCap<CNode<U4095, role::Child>>, _) =
            child_cnode_ut.retype_cnode::<_, U12>(root_cnode)?;

        let (child_ut6, child_cnode) = ut6.move_to_cnode(&root_cnode, child_cnode)?;

        let (child_cnode_as_child, child_cnode) =
            child_cnode.generate_self_reference(&root_cnode)?;

        (
            child_cnode,
            root_cnode,
            CapManagementParams {
                my_cnode: child_cnode_as_child,
                data_source: child_ut6,
            },
        )
    };
    #[cfg(test_case = "over_register_size_params")]
    let (child_cnode, root_cnode, params) = {
        let (child_cnode, root_cnode) = child_cnode_ut.retype_cnode::<_, U12>(root_cnode)?;

        let mut nums = [0xaaaaaaaa; 140];
        nums[0] = 0xbbbbbbbb;
        nums[139] = 0xcccccccc;
        (child_cnode, root_cnode, OverRegisterSizeParams { nums })
    };

    let (child_vspace, mut boot_info, root_cnode) =
        VSpace::new::<_, _, _, U1>(boot_info, child_vspace_ut, root_cnode, None)?;

    let (child_process, _caller_vspace, root_cnode) = child_vspace.prepare_thread(
        proc_main,
        params,
        child_thread_ut,
        root_cnode,
        &mut scratch_page_table,
        &mut boot_info.page_directory,
    )?;

    child_process.start(child_cnode, None, &boot_info.tcb, 255)?;

    Ok(())
}

pub struct ProcParams {
    pub value: usize,
}

impl RetypeForSetup for ProcParams {
    type Output = ProcParams;
}

#[cfg(test_case = "root_task_runs")]
pub extern "C" fn proc_main(_params: ProcParams) {}

#[cfg(test_case = "process_runs")]
pub extern "C" fn proc_main(params: ProcParams) {
    debug_println!("\nThe value inside the process is {}\n", unsafe {
        params.value
    });
}

#[cfg(test_case = "memory_read_protection")]
pub extern "C" fn proc_main(_params: ProcParams) {
    debug_println!("\nAttempting to cause a segmentation fault...\n");

    unsafe {
        let x: *const usize = 0x88888888usize as _;
        debug_println!("Value from arbitrary memory is: {}", *x);
    }

    debug_println!("This is after the segfaulting code, and should not be printed.");
}

#[cfg(test_case = "memory_write_protection")]
pub extern "C" fn proc_main(_params: ProcParams) {
    debug_println!("\nAttempting to write to the code segment...\n");

    unsafe {
        let x: *mut usize = proc_main as _;
        *x = 42;
    }

    debug_println!("This is after the segfaulting code, and should not be printed.");
}

#[derive(Debug)]
pub struct CapManagementParams<Role: CNodeRole> {
    pub my_cnode: Cap<CNode<Diff<Pow<U12>, U3>, Role>, Role>,
    pub data_source: Cap<Untyped<U6>, Role>,
}

impl RetypeForSetup for CapManagementParams<role::Local> {
    type Output = CapManagementParams<role::Child>;
}

// 'extern' to force C calling conventions
#[cfg(test_case = "child_process_cap_management")]
pub extern "C" fn proc_main(params: CapManagementParams<role::Local>) {
    debug_println!("");
    debug_println!("--- Hello from the cap_management_run feL4 process!");

    debug_println!("Let's split an untyped inside child process");
    let (ut_kid_a, ut_kid_b, cnode) = params
        .data_source
        .split(params.my_cnode)
        .expect("child process split untyped");
    debug_println!("We got past the split in a child process\n");

    debug_println!("Let's make an Endpoint");
    let (_endpoint, cnode): (LocalCap<Endpoint>, _) = ut_kid_a
        .retype_local(cnode)
        .expect("Retype local in a child process failure");
    debug_println!("Successfully built an Endpoint\n");

    debug_println!("And now for a delete in a child process");
    ut_kid_b.delete(&cnode).expect("child process delete a cap");
    debug_println!("Hey, we deleted a cap in a child process");
    debug_println!("Split, retyped, and deleted caps in a child process");
}

pub struct OverRegisterSizeParams {
    pub nums: [usize; 140],
}

impl RetypeForSetup for OverRegisterSizeParams {
    type Output = OverRegisterSizeParams;
}

#[cfg(test_case = "over_register_size_params")]
pub extern "C" fn proc_main(params: OverRegisterSizeParams) {
    debug_println!(
        "The child process saw a first value of {:08x}, a mid value of {:08x}, and a last value of {:08x}",
        params.nums[0],
        params.nums[70],
        params.nums[139]
    );
}
