use selfe_sys::seL4_BootInfo;

use typenum::*;

use ferros::alloc::{self, micro_alloc, smart_alloc};
use ferros::bootstrap::{root_cnode, BootInfo, UserImage};
use ferros::cap::{
    retype_cnode, role, ASIDPool, CNode, CNodeRole, CNodeSlotsData, Cap, ThreadPriorityAuthority,
    Untyped,
};
use ferros::userland::{CapRights, RetypeForSetup};
use ferros::vspace::{NewVSpaceCNodeSlots, VSpace, VSpaceScratchSlice};

use super::TopLevelError;

pub fn run(raw_boot_info: &'static seL4_BootInfo) -> Result<(), TopLevelError> {
    let BootInfo {
        root_page_directory,
        asid_control,
        user_image,
        root_tcb,
        ..
    } = BootInfo::wrap(&raw_boot_info);
    let mut allocator = micro_alloc::Allocator::bootstrap(&raw_boot_info)?;
    let (cnode, local_slots) = root_cnode(&raw_boot_info);
    let uts = alloc::ut_buddy(
        allocator
            .get_untyped::<U27>()
            .expect("initial alloc failure"),
    );

    smart_alloc!(|slots: local_slots, ut: uts| {
        let (mut local_vspace_scratch, _root_page_directory) =
            VSpaceScratchSlice::from_parts(slots, ut, root_page_directory)?;

        let (child_cnode, child_slots) = retype_cnode::<U20>(ut, slots)?;

        let (asid_pool_for_child, asid_control) = asid_control.allocate_asid_pool(ut, slots)?;
        let untyped_for_child = ut;
        let untyped_for_scratch = ut;
        let slots_for_scratch = slots;

        let (asid_pool, _asid_control) = asid_control.allocate_asid_pool(ut, slots)?;
        let (child_asid, _asid_pool) = asid_pool.alloc();
        let child_vspace = VSpace::new(ut, slots, child_asid, &user_image, &cnode)?;

        smart_alloc!(|slots_c: child_slots| {
            let (cnode_for_child, slots_for_child) =
                child_cnode.generate_self_reference(&cnode, slots_c)?;
            let untyped_for_child = untyped_for_child.move_to_slot(&cnode, slots_c)?;
            let asid_pool_for_child = asid_pool_for_child.move_to_slot(&cnode, slots_c)?;
            let user_image_for_child = user_image.copy(&cnode, slots_c)?;
            let (vspace_scratch_for_child, child_vspace) = child_vspace.create_child_scratch(
                untyped_for_scratch,
                slots_for_scratch,
                slots_c,
                &cnode,
            )?;
            let thread_priority_authority_for_child =
                root_tcb.as_ref().copy(&cnode, slots_c, CapRights::RWG)?;
        });

        let params = ChildParams {
            cnode: cnode_for_child,
            cnode_slots: slots_for_child,
            untyped: untyped_for_child,
            asid_pool: asid_pool_for_child,
            user_image: user_image_for_child,
            vspace_scratch: vspace_scratch_for_child,
            thread_priority_authority: thread_priority_authority_for_child,
        };

        let (child_process, _) = child_vspace.prepare_thread(
            child_main,
            params,
            ut,
            slots,
            &mut local_vspace_scratch,
        )?;
    });

    child_process.start(child_cnode, None, root_tcb.as_ref(), 255)?;

    Ok(())
}

#[derive(Debug)]
pub struct ChildParams<Role: CNodeRole> {
    cnode: Cap<CNode<Role>, Role>,
    cnode_slots: Cap<CNodeSlotsData<Sum<NewVSpaceCNodeSlots, U70>, Role>, Role>,
    untyped: Cap<Untyped<U25>, Role>,
    asid_pool: Cap<ASIDPool<U1024>, Role>,
    user_image: UserImage<Role>,
    vspace_scratch: VSpaceScratchSlice<Role>,
    thread_priority_authority: Cap<ThreadPriorityAuthority, Role>,
}

impl RetypeForSetup for ChildParams<role::Local> {
    type Output = ChildParams<role::Child>;
}

pub extern "C" fn child_main(params: ChildParams<role::Local>) {
    debug_println!("\nMade it to child process\n");
    child_run(params).expect("Error in child process");
}

fn child_run(params: ChildParams<role::Local>) -> Result<(), TopLevelError> {
    let ChildParams {
        cnode,
        cnode_slots,
        untyped,
        asid_pool,
        user_image,
        mut vspace_scratch,
        thread_priority_authority,
    } = params;
    let uts = alloc::ut_buddy(untyped);

    smart_alloc!(|slots: cnode_slots, ut: uts| {
        let (child_cnode, _child_slots) = retype_cnode::<U8>(ut, slots)?;
        let params = GrandkidParams { value: 42 };

        let (child_asid, _asid_pool) = asid_pool.alloc();
        let child_vspace = VSpace::new(ut, slots, child_asid, &user_image, &cnode)?;
        let (child_process, _) =
            child_vspace.prepare_thread(grandkid_main, params, ut, slots, &mut vspace_scratch)?;
    });
    child_process.start(child_cnode, None, &thread_priority_authority, 255)?;

    Ok(())
}

pub struct GrandkidParams {
    pub value: usize,
}

impl RetypeForSetup for GrandkidParams {
    type Output = GrandkidParams;
}

pub extern "C" fn grandkid_main(_params: GrandkidParams) {
    debug_println!("Grandkid process successfully ran")
}
