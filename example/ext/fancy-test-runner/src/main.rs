#![no_std]
#![feature(proc_macro_hygiene)]

mod error;

use arrayvec::ArrayVec;
use error::TopLevelError;
use ferros::alloc::*;
use ferros::bootstrap::*;
use ferros::cap::*;
use ferros::userland::*;
use ferros::vspace::*;
use ferros::*;
use selfe_arc;
use typenum::*;

extern "C" {
    static _selfe_arc_data_start: u8;
    static _selfe_arc_data_end: usize;
}

fn main() {
    let raw_bootinfo = unsafe { &*sel4_start::BOOTINFO };
    run(raw_bootinfo).expect("Failed to run root task setup");
}

fn run(raw_bootinfo: &'static selfe_sys::seL4_BootInfo) -> Result<(), TopLevelError> {
    let raw_bootinfo = unsafe { &*sel4_start::BOOTINFO };
    let (allocator, mut dev_allocator) = micro_alloc::bootstrap_allocators(&raw_bootinfo).unwrap();
    let mut allocator = WUTBuddy::from(allocator);

    let archive_slice: &[u8] = unsafe {
        core::slice::from_raw_parts(
            &_selfe_arc_data_start,
            &_selfe_arc_data_end as *const _ as usize - &_selfe_arc_data_start as *const _ as usize,
        )
    };

    //////////////////////////////////////////////
    // Resources for launching the test process //
    //////////////////////////////////////////////

    let (root_cnode, local_slots) = root_cnode(&raw_bootinfo);
    let (root_vspace_slots, local_slots): (LocalCNodeSlots<U100>, _) = local_slots.alloc();
    let (ut_slots, local_slots): (LocalCNodeSlots<U100>, _) = local_slots.alloc();
    let mut ut_slots = ut_slots.weaken();

    let BootInfo {
        mut root_vspace,
        asid_control,
        user_image,
        root_tcb,
        ..
    } = BootInfo::wrap(
        &raw_bootinfo,
        allocator.alloc_strong::<U16>(&mut ut_slots).unwrap(),
        root_vspace_slots,
    );

    let tpa = root_tcb.downgrade_to_thread_priority_authority();
    let uts = alloc::ut_buddy(allocator.alloc_strong::<U20>(&mut ut_slots).unwrap());

    smart_alloc!(|slots: local_slots, ut: uts| {
        let (asid_pool, _asid_control) = asid_control.allocate_asid_pool(ut, slots).unwrap();

        let ut_for_scratch: LocalCap<Untyped<U12>> = ut;
        let sacrificial_page = ut_for_scratch.retype(slots).unwrap();
        let reserved_for_scratch = root_vspace.reserve(sacrificial_page).unwrap();

        let weak_slots: LocalCNodeSlots<op!(U1 << U14)> = slots;
        let mut weak_slots = weak_slots.weaken();
    });

    // TODO figure out some nicer way to do this, obviously.
    // WeakASIDPool would be an easy place to start.
    let mut test_asids: ArrayVec<[LocalCap<UnassignedASID>; 16]> = ArrayVec::new();
    let (a, asid_pool) = asid_pool.alloc();
    test_asids.push(a);
    let (a, asid_pool) = asid_pool.alloc();
    test_asids.push(a);
    let (a, asid_pool) = asid_pool.alloc();
    test_asids.push(a);
    let (a, asid_pool) = asid_pool.alloc();
    test_asids.push(a);
    let (a, asid_pool) = asid_pool.alloc();
    test_asids.push(a);
    let (a, asid_pool) = asid_pool.alloc();
    test_asids.push(a);
    let (a, asid_pool) = asid_pool.alloc();
    test_asids.push(a);
    let (a, asid_pool) = asid_pool.alloc();
    test_asids.push(a);
    let (a, asid_pool) = asid_pool.alloc();
    test_asids.push(a);
    let (a, asid_pool) = asid_pool.alloc();
    test_asids.push(a);
    let (a, asid_pool) = asid_pool.alloc();
    test_asids.push(a);
    let (a, asid_pool) = asid_pool.alloc();
    test_asids.push(a);
    let (a, asid_pool) = asid_pool.alloc();
    test_asids.push(a);
    let (a, asid_pool) = asid_pool.alloc();
    test_asids.push(a);
    let (a, asid_pool) = asid_pool.alloc();
    test_asids.push(a);
    let (a, asid_pool) = asid_pool.alloc();
    test_asids.push(a);

    ///////////////////////////
    // Run each test process //
    ///////////////////////////

    let archive = selfe_arc::read::Archive::from_slice(archive_slice);
    for file in archive.all_files().unwrap() {
        let dir_entry = file.unwrap();
        debug_println!("[Test Runner] Testing {}", dir_entry.name().unwrap());

        let test_asid = test_asids.pop().unwrap();

        let stack_mem: UnmappedMemoryRegion<U18, _> = UnmappedMemoryRegion::new(
            allocator.alloc_strong(&mut weak_slots)?,
            weak_slots.alloc_strong()?,
        )
        .unwrap();

        let stack_mem = root_vspace
            .map_region(stack_mem, CapRights::RW, arch::vm_attributes::DEFAULT)
            .unwrap();

        // TODO fix selfe so we can get this directly from the dir entry
        let elf_data = archive.file(dir_entry.name().unwrap()).unwrap();

        let mut scratch = reserved_for_scratch.as_scratch(&mut root_vspace).unwrap();
        run_test_process(
            elf_data,
            // TODO re-use these, instead of burning resources
            allocator.alloc_strong(&mut weak_slots)?,
            weak_slots.alloc_strong()?,
            test_asid,
            &root_cnode,
            &user_image,
            &mut scratch,
            stack_mem,
            &tpa,
        )?;
    }


    debug_println!("[Test Runner] All tests complete");

    unsafe {
        loop {
            selfe_sys::seL4_Yield();
        }
    }

}

fn run_test_process<'a, 'b, 'c>(
    elf_data: &[u8],
    uts: LocalCap<Untyped<U20>>,
    local_slots: LocalCNodeSlots<U2048>,
    test_asid: LocalCap<UnassignedASID>,
    root_cnode: &LocalCap<LocalCNode>,
    user_image: &UserImage<role::Local>,
    mut scratch: &'a mut ScratchRegion<'b, 'c>,
    stack_mem: MappedMemoryRegion<U18, shared_status::Exclusive>,
    priority_authority: &LocalCap<ThreadPriorityAuthority>,
) -> Result<(), TopLevelError> {

    let uts = alloc::ut_buddy(uts);
    smart_alloc!(|slots: local_slots, ut: uts| {
        let vspace_slots: LocalCNodeSlots<U16> = slots;
        let vspace_ut: LocalCap<Untyped<U16>> = ut;

        let page_slots: LocalCNodeSlots<U1024> = slots;
        let elf_writable_mem: LocalCap<Untyped<U18>> = ut;

        let mut test_vspace = VSpace::new_from_elf_weak(
            retype(ut, slots).unwrap(), // paging_root
            test_asid,
            vspace_slots.weaken(), // slots
            vspace_ut.weaken(),    // paging_untyped
            &elf_data,
            page_slots.weaken(),
            elf_writable_mem.weaken(),
            &user_image,
            &root_cnode,
            &mut scratch,
        )
        .unwrap();

        let (test_cnode, test_slots) = retype_cnode::<U12>(ut, slots).unwrap();
        let (test_fault_source_slot, _test_slots) = test_slots.alloc();

        let (fault_source, test_event_sender, fault_or_event_handler) =
            fault_or_message_channel::<fancy_test::TestEvent, role::Local>(
                &root_cnode,
                ut,
                slots,
                test_fault_source_slot,
                slots,
            )?;

        let params = fancy_test::TestContext {
            x: 42,
            test_event_sender,
        };

        let test_process = StandardProcess::new::<fancy_test::TestContext<_>, _>(
            &mut test_vspace,
            test_cnode,
            stack_mem,
            &root_cnode,
            elf_data,
            params,
            ut, // ipc_buffer_ut
            ut, // tcb_ut
            slots,
            &priority_authority,
            Some(fault_source), // fault
        )
        .unwrap();
    });

    test_process.start().unwrap();

    let mut current_test: Option<fancy_test::TestName> = None;
    loop {
        match fault_or_event_handler.await_message()? {
            FaultOrMessage::Fault(_) => {
                debug_println!(
                    "\n[Test Runner] Test process faulted; last running test was '{}'",
                    current_test.unwrap_or_default()
                );
                break;
            }
            FaultOrMessage::Message(m) => match m {
                fancy_test::TestEvent::TestStarting(name) => {
                    current_test = Some(name);
                }
                fancy_test::TestEvent::TestPassed(name) => {
                    if let Some(current_name) = current_test {
                        if current_name == name {
                            current_test = None
                        } else {
                            panic!("Received 'passed' event for test '{}', but expected test '{}' to be running",
                                   name, current_name);
                        }
                    } else {
                        panic!("Received 'passed' event for test '{}', but expected no test to be running",
                               name);
                    }
                    current_test = None;
                }
                fancy_test::TestEvent::AllTestsComplete => {
                    debug_println!("[Test Runner] Test process complete");
                    break;
                }
            },
        }
    }

    Ok(())
}
