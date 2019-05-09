use super::TopLevelError;
use selfe_sys::seL4_BootInfo;

use ferros::alloc::{self, micro_alloc, smart_alloc};
use typenum::*;

use ferros::userland::{
    retype, retype_cnode, role, root_cnode, BootInfo, CNode, CNodeRole, CNodeSlotsData, Cap,
    Endpoint, LocalCap, RetypeForSetup, Untyped, VSpace,
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
            .get_untyped::<U27>()
            .expect("initial alloc failure"),
    );

    smart_alloc!(|slots from local_slots, ut from uts| {
        let unmapped_scratch_page_table = retype(ut, slots)?;
        let (mut scratch_page_table, mut root_page_directory) =
            root_page_directory.map_page_table(unmapped_scratch_page_table)?;

        let (asid_pool, _asid_control) = asid_control.allocate_asid_pool(ut, slots)?;
        let (child_asid, _asid_pool) = asid_pool.alloc();
        let (child_cnode, child_slots) = retype_cnode::<U12>(ut, slots)?;

        let ut5: LocalCap<Untyped<U5>> = ut;

        smart_alloc!(|slots_c from child_slots| {
            let (cnode_for_child, slots_for_child) = child_cnode.generate_self_reference(&root_cnode, slots_c)?;
            let child_ut5 = ut5.move_to_slot(&root_cnode, slots_c)?;
        });

        let params = CapManagementParams {
            my_cnode: cnode_for_child,
            my_cnode_slots: slots_for_child,
            my_ut: child_ut5,
        };

        let child_vspace = VSpace::new(ut, slots, child_asid, &user_image, &root_cnode)?;

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
pub extern "C" fn proc_main(params: CapManagementParams<role::Local>) {
    debug_println!("");
    debug_println!("--- Hello from the cap_management_run ferros process!");
    debug_println!("{:#?}", params);

    let CapManagementParams {
        my_cnode,
        my_cnode_slots,
        my_ut,
    } = params;

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
