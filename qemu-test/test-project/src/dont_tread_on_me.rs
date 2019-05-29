//! A test verifying that, should a process need a writable copy of
//! the user image, that such a write cannot affect another process'
//! copy of the user image.
use core::ptr;

use ferros::alloc::{self, micro_alloc, smart_alloc};
use ferros::bootstrap::{root_cnode, BootInfo};
use ferros::cap::{retype_cnode, role, CNodeRole};
use ferros::userland::{call_channel, Caller, Responder, RetypeForSetup};
use ferros::vspace::{VSpace, VSpaceScratchSlice};

use typenum::*;

use selfe_sys::seL4_BootInfo;

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
    let (root_cnode, local_slots) = root_cnode(&raw_boot_info);
    let uts = alloc::ut_buddy(
        allocator
            .get_untyped::<U27>()
            .expect("initial alloc failure"),
    );

    smart_alloc!(|slots: local_slots, ut: uts| {
        let (mut local_vspace_scratch, _root_page_directory) =
            VSpaceScratchSlice::from_parts(slots, ut, root_page_directory)?;

        let (proc1_cspace, proc1_slots) = retype_cnode::<U12>(ut, slots)?;
        let (proc2_cspace, proc2_slots) = retype_cnode::<U12>(ut, slots)?;

        let (asid_pool, _asid_control) = asid_control.allocate_asid_pool(ut, slots)?;
        let (proc1_asid, asid_pool) = asid_pool.alloc();
        let (proc2_asid, _asid_pool) = asid_pool.alloc();

        let proc1_vspace = VSpace::new(ut, slots, proc1_asid, &user_image, &root_cnode)?;
        let proc2_vspace = VSpace::new_with_writable_user_image(
            ut,
            slots,
            proc2_asid,
            &user_image,
            &root_cnode,
            (&mut local_vspace_scratch, ut),
        )?;

        let (slots1, _) = proc1_slots.alloc();
        let (ipc_setup, responder) = call_channel(ut, &root_cnode, slots, slots1)?;

        let (slots2, _) = proc2_slots.alloc();
        let caller = ipc_setup.create_caller(slots2)?;

        let proc1_params = proc1::Proc1Params { rspdr: responder };
        let proc2_params = proc2::Proc2Params { cllr: caller };

        let (proc1_thread, _) = proc1_vspace.prepare_thread(
            proc1::run,
            proc1_params,
            ut,
            slots,
            &mut local_vspace_scratch,
        )?;

        proc1_thread.start(proc1_cspace, None, root_tcb.as_ref(), 255)?;

        let (proc2_thread, _) = proc2_vspace.prepare_thread(
            proc2::run,
            proc2_params,
            ut,
            slots,
            &mut local_vspace_scratch,
        )?;

        proc2_thread.start(proc2_cspace, None, root_tcb.as_ref(), 255)?;
    });

    Ok(())
}

#[allow(dead_code)]
fn to_be_changed() {
    debug_println!("not changed at all");
}

pub mod proc1 {
    use super::*;

    pub struct Proc1Params<Role: CNodeRole> {
        pub rspdr: Responder<(), (), Role>,
    }

    impl RetypeForSetup for Proc1Params<role::Local> {
        type Output = Proc1Params<role::Child>;
    }

    pub extern "C" fn run(params: Proc1Params<role::Local>) {
        params
            .rspdr
            .reply_recv(|_| {
                to_be_changed();
            })
            .expect("reply recv blew up");
    }
}

pub mod proc2 {
    use super::*;

    pub struct Proc2Params<Role: CNodeRole> {
        pub cllr: Caller<(), (), Role>,
    }

    impl RetypeForSetup for Proc2Params<role::Local> {
        type Output = Proc2Params<role::Child>;
    }

    pub extern "C" fn run(params: Proc2Params<role::Local>) {
        unsafe {
            let tbc_ptr = to_be_changed as *mut usize;
            ptr::write_volatile(tbc_ptr, 42);
        }
        params
            .cllr
            .blocking_call(&())
            .expect("blocking call blew up");
    }
}