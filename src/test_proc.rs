use core::marker::PhantomData;
use crate::userland::{role, Badge, CNodeRole, FaultSink, RetypeForSetup};
use sel4_sys::{seL4_MessageInfo_new, seL4_Send};

#[derive(Debug)]
pub struct CapFaulterParams<Role: CNodeRole> {
    pub _role: PhantomData<Role>,
}

impl RetypeForSetup for CapFaulterParams<role::Local> {
    type Output = CapFaulterParams<role::Child>;
}

#[derive(Debug)]
pub struct VMFaulterParams<Role: CNodeRole> {
    pub _role: PhantomData<Role>,
}

impl RetypeForSetup for VMFaulterParams<role::Local> {
    type Output = VMFaulterParams<role::Child>;
}

#[derive(Debug)]
pub struct MischiefDetectorParams<Role: CNodeRole> {
    pub fault_sink: FaultSink<Role>,
}

impl RetypeForSetup for MischiefDetectorParams<role::Local> {
    type Output = MischiefDetectorParams<role::Child>;
}

pub extern "C" fn vm_fault_source_proc(_p: VMFaulterParams<role::Local>) {
    debug_println!("Inside vm_fault_source_proc");
    debug_println!("Attempting to cause a segmentation fault");
    unsafe {
        let x: *const usize = 0x88888888usize as _;
        let y = *x;
        debug_println!(
            "Value from arbitrary memory is: {}, but we shouldn't get far enough to print it",
            y
        );
    }
    debug_println!("This is after the vm fault inducing code, and should not be printed.");
}

pub extern "C" fn cap_fault_source_proc(_p: CapFaulterParams<role::Local>) {
    debug_println!("Inside cap_fault_source_proc");
    debug_println!("\nAttempting to cause a cap fault");
    unsafe {
        seL4_Send(
            314159, // bogus cptr to nonexistent endpoint
            seL4_MessageInfo_new(0, 0, 0, 0),
        )
    }
    debug_println!("This is after the capability fault inducing code, and should not be printed.");
}

pub extern "C" fn fault_sink_proc(p: MischiefDetectorParams<role::Local>) {
    debug_println!("Inside fault_sink_proc");
    let mut last_sender = Badge::from(0usize);
    for i in 1..=2 {
        let fault = p.fault_sink.wait_for_fault();
        debug_println!("Caught fault {}: {:?}", i, fault);
        if last_sender == fault.sender() {
            debug_println!("Fault badges were equal, but we wanted distinct senders");
            panic!("Stop the presses")
        }
        last_sender = fault.sender();
    }
    debug_println!("Successfully caught 2 faults from 2 distinct sources");
}
