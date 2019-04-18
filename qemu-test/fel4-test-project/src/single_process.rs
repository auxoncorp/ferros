use super::TopLevelError;

use ferros::alloc::{self, smart_alloc, micro_alloc};
use typenum::*;

use ferros::pow::Pow;
use ferros::userland::{
    role, root_cnode, BootInfo, CNode, CNodeRole, Cap, Endpoint, LocalCap, RetypeForSetup,
    SeL4Error, UnmappedPageTable, Untyped, VSpace, retype, retype_cnode, CNodeSlots, CNodeSlotsData
};
use sel4_sys::*;
type U4095 = Diff<U4096, U1>;

pub fn run(raw_boot_info: &'static seL4_BootInfo) -> Result<(), TopLevelError> {
    #[cfg(test_case = "root_task_runs")]
    {
        debug_println!("\nhello from the root task!\n");
    }

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

        let (child_cnode, child_slots) = retype_cnode::<U12>(ut, slots)?;
    });


    #[cfg(min_params = "true")]
    smart_alloc!(|slots from local_slots, ut from uts| {
        let params = ProcParams { value: 42 };
    });

    #[cfg(test_case = "child_process_cap_management")]
    smart_alloc!(|slots from local_slots| {
        let (ut5, uts): (LocalCap<Untyped<U5>>, _) = uts.alloc(slots)?;

        smart_alloc!(|slots_c from child_slots| {
            let (cnode_for_child, slots_for_child) = child_cnode.generate_self_reference(&root_cnode, slots_c)?;
            let child_ut5 = ut5.move_to_cnode(&root_cnode, slots_c)?;
        });

        let params = CapManagementParams {
            my_cnode: cnode_for_child,
            my_cnode_slots: slots_for_child,
            my_ut: child_ut5,
        };
    });

    #[cfg(test_case = "over_register_size_params")]
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
    pub my_cnode: Cap<CNode<Role>, Role>,
    pub my_cnode_slots: Cap<CNodeSlotsData<U42, Role>, Role>,
    pub my_ut: Cap<Untyped<U5>, Role>,
}

impl RetypeForSetup for CapManagementParams<role::Local> {
    type Output = CapManagementParams<role::Child>;
}

// 'extern' to force C calling conventions
#[cfg(test_case = "child_process_cap_management")]
pub extern "C" fn proc_main(params: CapManagementParams<role::Local>) {
    debug_println!("");
    debug_println!("--- Hello from the cap_management_run feL4 process!");
    debug_println!("{:#?}", params);

    let CapManagementParams { my_cnode, my_cnode_slots, my_ut } = params;

    smart_alloc!(|slots from my_cnode_slots| {
        debug_println!("Let's split an untyped inside child process");
        let (ut_kid_a, ut_kid_b) = my_ut
            .split(slots)
            .expect("child process split untyped");
        debug_println!("We got past the split in a child process\n");

        debug_println!("Let's make an Endpoint");
        let _endpoint: LocalCap<Endpoint> = retype(ut_kid_a, slots).expect("Retype local in a child process failure");
        debug_println!("Successfully built an Endpoint\n");

        debug_println!("And now for a delete in a child process");
        ut_kid_b.delete(&my_cnode).expect("child process delete a cap");
        debug_println!("Hey, we deleted a cap in a child process");
        debug_println!("Split, retyped, and deleted caps in a child process");
    });
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
