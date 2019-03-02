use super::TopLevelError;
use core::marker::PhantomData;
use ferros::micro_alloc;
use ferros::pow::Pow;
use ferros::userland::{
    call_channel, role, root_cnode, setup_fault_endpoint_pair, BootInfo, CNode, CNodeRole, Caller,
    Cap, Consumer1, Endpoint, FaultSink, LocalCap, Producer, ProducerSetup, QueueFullError,
    Responder, RetypeForSetup, SeL4Error, UnmappedPageTable, Untyped, VSpace,
};
use sel4_sys::*;
use typenum::{Diff, U1, U100, U12, U2, U20, U3, U4096, U6};
type U4095 = Diff<U4096, U1>;

use sel4_sys::seL4_Yield;

pub fn run(raw_boot_info: &'static seL4_BootInfo) -> Result<(), TopLevelError> {
    // wrap all untyped memory
    let mut allocator = micro_alloc::Allocator::bootstrap(&raw_boot_info)?;

    // wrap root CNode for safe usage
    let root_cnode = root_cnode(&raw_boot_info);

    // find an untyped of size 20 bits (1 meg)
    let ut20 = allocator
        .get_untyped::<U20>()
        .expect("Couldn't find initial untyped");

    let (ut18, ut18b, child_a_ut18, child_b_ut18, root_cnode) = ut20.quarter(root_cnode)?;

    let (child_a_vspace_ut, child_a_thread_ut, root_cnode) = child_a_ut18.split(root_cnode)?;
    let (child_b_vspace_ut, child_b_thread_ut, root_cnode) = child_b_ut18.split(root_cnode)?;

    let (ut16a, ut16b, _, _, root_cnode) = ut18.quarter(root_cnode)?;
    let (ut16e, _, _, _, root_cnode) = ut18b.quarter(root_cnode)?;
    let (ut14, _, _, _, root_cnode) = ut16e.quarter(root_cnode)?;
    let (ut12, asid_pool_ut, shared_page_ut, _, root_cnode) = ut14.quarter(root_cnode)?;
    let (ut10, scratch_page_table_ut, _, _, root_cnode) = ut12.quarter(root_cnode)?;
    let (ut8, _, _, _, root_cnode) = ut10.quarter(root_cnode)?;
    let (ut6, _, _, _, root_cnode) = ut8.quarter(root_cnode)?;
    let (ut4, _, _, _, root_cnode) = ut6.quarter(root_cnode)?;

    // wrap the rest of the critical boot info
    let (mut boot_info, root_cnode) = BootInfo::wrap(raw_boot_info, asid_pool_ut, root_cnode);

    let (scratch_page_table, root_cnode): (LocalCap<UnmappedPageTable>, _) =
        scratch_page_table_ut.retype_local(root_cnode)?;
    let (mut scratch_page_table, mut boot_info) = boot_info.map_page_table(scratch_page_table)?;

    let (child_a_vspace, mut boot_info, root_cnode) =
        VSpace::new(boot_info, child_a_vspace_ut, root_cnode)?;
    let (child_b_vspace, mut boot_info, root_cnode) =
        VSpace::new(boot_info, child_b_vspace_ut, root_cnode)?;

    let (mischief_maker_cnode, root_cnode): (LocalCap<CNode<U4095, role::Child>>, _) =
        ut16a.retype_cnode::<_, U12>(root_cnode)?;

    let (fault_handler_cnode, root_cnode): (LocalCap<CNode<U4095, role::Child>>, _) =
        ut16b.retype_cnode::<_, U12>(root_cnode)?;

    let (mischief_maker_cnode, fault_handler_cnode, fault_source, fault_sink, root_cnode) =
        setup_fault_endpoint_pair(root_cnode, ut4, mischief_maker_cnode, fault_handler_cnode)?;

    let mischief_maker_params = MischiefMakerParams { _role: PhantomData };

    let fault_handler_params = MischiefDetectorParams::<role::Child> { fault_sink };
    let (mischief_maker_process, _caller_vspace, root_cnode) = child_a_vspace.prepare_thread(
        mischief_maker_proc,
        mischief_maker_params,
        child_a_thread_ut,
        root_cnode,
        &mut scratch_page_table,
        &mut boot_info.page_directory,
    )?;
    mischief_maker_process.start(
        mischief_maker_cnode,
        Some(fault_source),
        &boot_info.tcb,
        255,
    )?;

    let (fault_handler_process, _caller_vspace, root_cnode) = child_b_vspace.prepare_thread(
        fault_handler_proc,
        fault_handler_params,
        child_b_thread_ut,
        root_cnode,
        &mut scratch_page_table,
        &mut boot_info.page_directory,
    )?;
    fault_handler_process.start(fault_handler_cnode, None, &boot_info.tcb, 255)?;

    Ok(())
}

#[derive(Debug)]
pub struct MischiefMakerParams<Role: CNodeRole> {
    pub _role: PhantomData<Role>,
}

impl RetypeForSetup for MischiefMakerParams<role::Local> {
    type Output = MischiefMakerParams<role::Child>;
}

#[derive(Debug)]
pub struct MischiefDetectorParams<Role: CNodeRole> {
    pub fault_sink: FaultSink<Role>,
}

impl RetypeForSetup for MischiefDetectorParams<role::Local> {
    type Output = MischiefDetectorParams<role::Child>;
}

pub extern "C" fn mischief_maker_proc(_p: MischiefMakerParams<role::Local>) {
    debug_println!("Inside fault_source_proc");
    debug_println!("\nAttempting to cause a cap fault");
    unsafe {
        seL4_Send(
            314159, // bogus cptr to nonexistent endpoint
            seL4_MessageInfo_new(0, 0, 0, 0),
        )
    }
    debug_println!("This is after the capability fault inducing code, and should not be printed.");
}

pub extern "C" fn fault_handler_proc(p: MischiefDetectorParams<role::Local>) {
    debug_println!("Inside fault_sink_proc");
    let fault = p.fault_sink.wait_for_fault();
    debug_println!("Caught a fault: {:?}", fault);
}
