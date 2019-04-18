use super::TopLevelError;
use core::marker::PhantomData;
use ferros::alloc::{self, smart_alloc, micro_alloc};
use ferros::pow::Pow;
use ferros::userland::{
    call_channel, role, root_cnode, setup_fault_endpoint_pair, BootInfo, CNode, CNodeRole, Caller,
    Cap, Consumer1, Endpoint, FaultSink, LocalCap, Producer, ProducerSetup, QueueFullError,
    Responder, RetypeForSetup, SeL4Error, UnmappedPageTable, Untyped, VSpace, retype, retype_cnode
};
use sel4_sys::*;
use typenum::*;
type U4095 = Diff<U4096, U1>;

use sel4_sys::seL4_Yield;

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

        let (mischief_maker_vspace, mut boot_info) = VSpace::new(boot_info, ut, &root_cnode, slots)?;
        let (mischief_maker_cnode, mischief_maker_slots) = retype_cnode::<U12>(ut, slots)?;

        let (fault_handler_vspace, mut boot_info) = VSpace::new(boot_info, ut, &root_cnode, slots)?;
        let (fault_handler_cnode, fault_handler_slots) = retype_cnode::<U12>(ut, slots)?;

        let (slots_source, mischief_maker_slots) = mischief_maker_slots.alloc();
        let (slots_sink, fault_handler_slots) = fault_handler_slots.alloc();
        let (fault_source, fault_sink) =
            setup_fault_endpoint_pair(&root_cnode, ut, slots, slots_source, slots_sink)?;

        let mischief_maker_params = MischiefMakerParams { _role: PhantomData };
        let fault_handler_params = MischiefDetectorParams::<role::Child> { fault_sink };

        let (mischief_maker_process, _) = mischief_maker_vspace.prepare_thread(
            mischief_maker_proc,
            mischief_maker_params,
            ut,
            slots,
            &mut scratch_page_table,
            &mut boot_info.page_directory,
        )?;

        mischief_maker_process.start(
            mischief_maker_cnode,
            Some(fault_source),
            &boot_info.tcb,
            255,
        )?;


        let (fault_handler_process, _) = fault_handler_vspace.prepare_thread(
            fault_handler_proc,
            fault_handler_params,
            ut,
            slots,
            &mut scratch_page_table,
            &mut boot_info.page_directory,
        )?;

        fault_handler_process.start(
            fault_handler_cnode,
            None,
            &boot_info.tcb,
            255
        )?;
    });

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
