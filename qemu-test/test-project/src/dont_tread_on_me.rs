//! A test verifying that, should a process need a writable copy of
//! the user image, that such a write cannot affect another process'
//! copy of the user image.


use ferros::alloc::{self, smart_alloc, micro_alloc};
use ferros::userland::{call_channel, root_cnode, BootInfo, VSpace, retype, retype_cnode};

use typenum::*;

use selfe_sys::{seL4_BootInfo};

use super::TopLevelError;

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
        debug_println!("page table retyped");
        let (mut scratch_page_table, boot_info) =
            boot_info.map_page_table(unmapped_scratch_page_table)?;

        let (proc1_cspace, proc1_slots) = retype_cnode::<U12>(ut, slots)?;
        debug_println!("proc 1 cspace retyped");
        let (proc2_cspace, proc2_slots) = retype_cnode::<U12>(ut, slots)?;
        debug_println!("proc 2 cspace retyped");

        let (proc1_vspace, mut boot_info) = VSpace::new(boot_info, ut, &root_cnode, slots)?;
        debug_println!("proc 1 vspace exists");

        let (proc2_vspace, mut boot_info) = VSpace::new_with_writable_user_image(
            boot_info,
            ut,
            (&mut scratch_page_table, ut),
            &root_cnode,
            slots
        )?;
        debug_println!("proc 2 vspace exists");

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
            &mut scratch_page_table,
            &mut boot_info.page_directory,
        )?;

        proc1_thread.start(proc1_cspace, None, &boot_info.tcb, 255)?;

        let (proc2_thread, _) = proc2_vspace.prepare_thread(
            proc2::run,
            proc2_params,
            ut,
            slots,
            &mut scratch_page_table,
            &mut boot_info.page_directory,
        )?;

        proc2_thread.start(proc2_cspace, None, &boot_info.tcb, 255)?;
    });

    Ok(())
}

fn to_be_changed() {
    debug_println!("not changed at all");
}

pub mod proc1 {
    use ferros::userland::{role, CNodeRole, Responder, RetypeForSetup};

    use super::to_be_changed;

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
    use core::ptr;
    use ferros::userland::{role, CNodeRole, Caller, RetypeForSetup};

    use super::to_be_changed;

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
