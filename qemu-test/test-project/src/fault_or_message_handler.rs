use selfe_sys::{seL4_MessageInfo_new, seL4_Send};

use ferros::alloc::{smart_alloc, ut_buddy};
use ferros::bootstrap::*;
use ferros::cap::*;
use ferros::test_support::*;
use ferros::userland::*;
use ferros::vspace::*;

use typenum::*;

use super::TopLevelError;

use ferros_test::ferros_test;

type U33768 = Sum<U32768, U1000>;

#[ferros_test]
pub fn fault_or_message_handler(
    mut outer_slots: LocalCNodeSlots<U33768>,
    mut outer_ut: LocalCap<Untyped<U21>>,
    mut asid_pool: LocalCap<ASIDPool<U1024>>,
    local_vspace_scratch: &mut VSpaceScratchSlice<role::Local>,
    root_cnode: &LocalCap<LocalCNode>,
    user_image: &UserImage<role::Local>,
    tpa: &LocalCap<ThreadPriorityAuthority>,
) -> Result<(), TopLevelError> {
    for c in [
        Command::ReportTrue,
        Command::ReportFalse,
        Command::ThrowFault,
        Command::ReportTrue,
        Command::ThrowFault,
        Command::ReportFalse,
    ]
    .iter()
    .cycle()
    .take(6)
    {
        with_temporary_resources(
            &mut outer_slots,
            &mut outer_ut,
            &mut asid_pool,
            |inner_slots, inner_ut, inner_asid_pool| -> Result<(), TopLevelError> {
                let uts = ut_buddy(inner_ut);
                smart_alloc!(|slots: inner_slots, ut: uts| {
                    let (child_cnode, child_slots) = retype_cnode::<U12>(ut, slots)?;
                    let (child_fault_source_slot, _child_slots) = child_slots.alloc();
                    let (source, sender, handler) = fault_or_message_channel(
                        &root_cnode,
                        ut,
                        slots,
                        child_fault_source_slot,
                        slots,
                    )?;
                    let params = ProcParams {
                        command: c.clone(),
                        sender,
                    };

                    let (child_asid, _asid_pool) = inner_asid_pool.alloc();
                    let child_vspace =
                        VSpace::new(ut, slots, child_asid, &user_image, &root_cnode)?;

                    let (child_process, _) = child_vspace.prepare_thread(
                        proc_main,
                        params,
                        ut,
                        slots,
                        local_vspace_scratch,
                    )?;
                });
                child_process.start(child_cnode, Some(source), tpa, 255)?;

                match handler.await_message()? {
                    FaultOrMessage::Fault(_) => {
                        if c != &Command::ThrowFault {
                            panic!("Child process threw a fault when it should not have")
                        }
                    }
                    FaultOrMessage::Message(m) => match c {
                        Command::ThrowFault => {
                            panic!("Command expected a fault to be thrown, not a message sent")
                        }
                        Command::ReportTrue => {
                            assert_eq!(true, m, "Command expected success true to be reported")
                        }
                        Command::ReportFalse => {
                            assert_eq!(false, m, "Command expected success false to be reported")
                        }
                    },
                }
                Ok(())
            },
        )??;
    }
    Ok(())
}

#[derive(Debug, Clone, Copy, PartialEq)]
#[repr(C)]
pub enum Command {
    ReportTrue,
    ReportFalse,
    ThrowFault,
}

pub struct ProcParams<Role: CNodeRole> {
    pub command: Command,
    pub sender: Sender<bool, Role>,
}

impl RetypeForSetup for ProcParams<role::Local> {
    type Output = ProcParams<role::Child>;
}

pub extern "C" fn proc_main(params: ProcParams<role::Local>) {
    let ProcParams { command, sender } = params;
    match command {
        Command::ReportTrue => sender.blocking_send(&true).expect("Could not send true"),
        Command::ReportFalse => sender.blocking_send(&false).expect("Could not send false"),
        Command::ThrowFault => {
            unsafe {
                seL4_Send(
                    314159, // bogus cptr to nonexistent endpoint
                    seL4_MessageInfo_new(0, 0, 0, 0),
                );
            }
        }
    }
}
