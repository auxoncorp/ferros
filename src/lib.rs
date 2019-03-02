#![no_std]
#![cfg_attr(feature = "alloc", feature(alloc))]
// Necessary to mark as not-Send or not-Sync
#![feature(optin_builtin_traits)]
#![feature(associated_type_defaults)]
#![recursion_limit = "128"]

#[cfg(all(feature = "alloc"))]
#[macro_use]
extern crate alloc;

extern crate arrayvec;
extern crate generic_array;
extern crate registers;
extern crate sel4_sys;
extern crate typenum;

extern crate cross_queue;

#[cfg(all(feature = "test"))]
extern crate proptest;

#[cfg(feature = "test")]
pub mod fel4_test;

#[macro_use]
mod debug;

pub mod drivers;

pub mod micro_alloc;
pub mod pow;
mod twinkle_types;
pub mod userland;

use crate::micro_alloc::Error as AllocError;
use crate::userland::{
    call_channel, root_cnode, BootInfo, IPCError, IRQError, MultiConsumerError, SeL4Error, VSpace,
    VSpaceError,
};

use sel4_sys::*;
use typenum::{U12, U27};

fn yield_forever() {
    unsafe {
        loop {
            seL4_Yield();
        }
    }
}

pub fn run(raw_boot_info: &'static seL4_BootInfo) {
    do_run(raw_boot_info).expect("run error");
    yield_forever();
}

fn do_run(raw_boot_info: &'static seL4_BootInfo) -> Result<(), TopLevelError> {
    let mut allocator = micro_alloc::Allocator::bootstrap(&raw_boot_info)?;
    let root_cnode = root_cnode(&raw_boot_info);

    let ut27 = allocator
        .get_untyped::<U27>()
        .expect("initial alloc failure");

    let (ui_ut, ut26, root_cnode) = ut27.split(root_cnode)?;
    let (ut24, _, _, _, root_cnode) = ut26.quarter(root_cnode)?;
    let (ut22, _, _, _, root_cnode) = ut24.quarter(root_cnode)?;
    let (ut20, _, _, _, root_cnode) = ut22.quarter(root_cnode)?;
    let (ut18a, ut18b, ut18c, _, root_cnode) = ut20.quarter(root_cnode)?;
    let (proc1_vspace_ut, proc1_thread_ut, root_cnode) = ut18a.split(root_cnode)?;
    let (proc2_vspace_ut, proc2_thread_ut, root_cnode) = ut18b.split(root_cnode)?;

    let (proc1_cspace_ut, proc2_cspace_ut, ut16, _, root_cnode) = ut18c.quarter(root_cnode)?;
    let (ut14, _, _, _, root_cnode) = ut16.quarter(root_cnode)?;
    let (asid_pool_ut, ut12, _, _, root_cnode) = ut14.quarter(root_cnode)?;
    let (scratch_page_table_ut, ut10, _, _, root_cnode) = ut12.quarter(root_cnode)?;
    let (ut8, _, _, _, root_cnode) = ut10.quarter(root_cnode)?;
    let (ut6, _, _, _, root_cnode) = ut8.quarter(root_cnode)?;
    let (endpoint_ut, _, _, _, root_cnode) = ut6.quarter(root_cnode)?;

    let (boot_info, root_cnode) = BootInfo::wrap(raw_boot_info, asid_pool_ut, root_cnode);

    let (unmapped_scratch_page_table, root_cnode) =
        scratch_page_table_ut.retype_local(root_cnode)?;
    debug_println!("page table retyped");
    let (mut scratch_page_table, boot_info) =
        boot_info.map_page_table(unmapped_scratch_page_table)?;

    let (proc1_cspace, root_cnode) = proc1_cspace_ut.retype_cnode::<_, U12>(root_cnode)?;
    debug_println!("proc 1 cspace retyped");
    let (proc2_cspace, root_cnode) = proc2_cspace_ut.retype_cnode::<_, U12>(root_cnode)?;
    debug_println!("proc 2 cspace retyped");

    let (proc1_vspace, boot_info, root_cnode) =
        VSpace::new(boot_info, proc1_vspace_ut, root_cnode)?;
    debug_println!("proc 1 vspace exists");
    let (proc2_vspace, mut boot_info, root_cnode) = VSpace::new_with_writable_user_image(
        boot_info,
        proc2_vspace_ut,
        (&mut scratch_page_table, ui_ut),
        root_cnode,
    )?;
    debug_println!("proc 2 vspace exists");

    let (ipc_setup, responder, proc1_cspace, root_cnode) =
        call_channel(endpoint_ut, proc1_cspace, root_cnode)?;

    let (caller, proc2_cspace) = ipc_setup.create_caller(proc2_cspace)?;

    let proc1_params = proc1::Proc1Params { rspdr: responder };

    let proc2_params = proc2::Proc2Params { cllr: caller };

    let (proc1_thread, _, root_cnode) = proc1_vspace.prepare_thread(
        proc1::run,
        proc1_params,
        proc1_thread_ut,
        root_cnode,
        &mut scratch_page_table,
        &mut boot_info.page_directory,
    )?;

    proc1_thread.start(proc1_cspace, None, &boot_info.tcb, 255)?;

    let (proc2_thread, _, _) = proc2_vspace.prepare_thread(
        proc2::run,
        proc2_params,
        proc2_thread_ut,
        root_cnode,
        &mut scratch_page_table,
        &mut boot_info.page_directory,
    )?;

    proc2_thread.start(proc2_cspace, None, &boot_info.tcb, 255)?;
    Ok(())
}

fn to_be_changed() {
    debug_println!("not changed at all");
}

pub mod proc1 {
    use crate::userland::{role, CNodeRole, Responder, RetypeForSetup};

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
    use crate::userland::{role, CNodeRole, Caller, RetypeForSetup};

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

#[derive(Debug)]
enum TopLevelError {
    AllocError(AllocError),
    IPCError(IPCError),
    IRQError(IRQError),
    MultiConsumerError(MultiConsumerError),
    SeL4Error(SeL4Error),
    VSpaceError(VSpaceError),
}

impl From<AllocError> for TopLevelError {
    fn from(e: AllocError) -> Self {
        TopLevelError::AllocError(e)
    }
}

impl From<IPCError> for TopLevelError {
    fn from(e: IPCError) -> Self {
        TopLevelError::IPCError(e)
    }
}

impl From<MultiConsumerError> for TopLevelError {
    fn from(e: MultiConsumerError) -> Self {
        TopLevelError::MultiConsumerError(e)
    }
}

impl From<VSpaceError> for TopLevelError {
    fn from(e: VSpaceError) -> Self {
        TopLevelError::VSpaceError(e)
    }
}

impl From<SeL4Error> for TopLevelError {
    fn from(e: SeL4Error) -> Self {
        TopLevelError::SeL4Error(e)
    }
}

impl From<IRQError> for TopLevelError {
    fn from(e: IRQError) -> Self {
        TopLevelError::IRQError(e)
    }
}
