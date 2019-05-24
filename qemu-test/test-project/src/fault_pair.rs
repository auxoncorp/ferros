use core::marker::PhantomData;

use selfe_sys::*;

use typenum::*;

use ferros::alloc::{self, micro_alloc, smart_alloc};
use ferros::bootstrap::{root_cnode, BootInfo};
use ferros::cap::{retype_cnode, role, CNodeRole};
use ferros::userland::{setup_fault_endpoint_pair, FaultSink, RetypeForSetup};
use ferros::vspace::{VSpace, VSpaceScratchSlice};

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

        let (asid_pool, _asid_control) = asid_control.allocate_asid_pool(ut, slots)?;

        let (mischief_maker_asid, asid_pool) = asid_pool.alloc();
        let mischief_maker_vspace =
            VSpace::new(ut, slots, mischief_maker_asid, &user_image, &root_cnode)?;
        let (mischief_maker_cnode, mischief_maker_slots) = retype_cnode::<U12>(ut, slots)?;

        let (fault_handler_asid, _asid_pool) = asid_pool.alloc();
        let fault_handler_vspace =
            VSpace::new(ut, slots, fault_handler_asid, &user_image, &root_cnode)?;
        let (fault_handler_cnode, fault_handler_slots) = retype_cnode::<U12>(ut, slots)?;

        let (slots_source, _mischief_maker_slots) = mischief_maker_slots.alloc();
        let (slots_sink, _fault_handler_slots) = fault_handler_slots.alloc();
        let (fault_source, fault_sink) =
            setup_fault_endpoint_pair(&root_cnode, ut, slots, slots_source, slots_sink)?;

        let mischief_maker_params = MischiefMakerParams { _role: PhantomData };
        let fault_handler_params = MischiefDetectorParams::<role::Child> { fault_sink };

        let (mischief_maker_process, _) = mischief_maker_vspace.prepare_thread(
            mischief_maker_proc,
            mischief_maker_params,
            ut,
            slots,
            &mut local_vspace_scratch,
        )?;

        mischief_maker_process.start(
            mischief_maker_cnode,
            Some(fault_source),
            root_tcb.as_ref(),
            255,
        )?;

        let (fault_handler_process, _) = fault_handler_vspace.prepare_thread(
            fault_handler_proc,
            fault_handler_params,
            ut,
            slots,
            &mut local_vspace_scratch,
        )?;

        fault_handler_process.start(fault_handler_cnode, None, root_tcb.as_ref(), 255)?;
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
