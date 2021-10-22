use selfe_sys::{seL4_MessageInfo_new, seL4_Send};

use ferros::alloc::{smart_alloc, ut_buddy};
use ferros::bootstrap::*;
use ferros::cap::*;
use ferros::test_support::*;
use ferros::userland::*;
use ferros::vspace::*;

use typenum::*;

use super::TopLevelError;

#[ferros_test::ferros_test]
pub fn fault_or_message_handler(
    mut outer_slots: LocalCNodeSlots<U32768>,
    mut outer_ut: LocalCap<Untyped<U21>>,
    mut asid_pool: LocalCap<ASIDPool<U512>>,
    mut irq_control: LocalCap<IRQControl>,
    mut local_mapped_region: MappedMemoryRegion<U17, shared_status::Exclusive>,
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
            &mut local_mapped_region,
            &mut irq_control,
            |inner_slots,
             inner_ut,
             inner_asid_pool,
             mapped_region,
             _inner_irq_control|
             -> Result<(), TopLevelError> {
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

                    let child_root = retype(ut, slots)?;
                    let child_vspace_slots: LocalCNodeSlots<U1024> = slots;
                    let child_vspace_ut: LocalCap<Untyped<U15>> = ut;

                    let mut child_vspace = VSpace::new(
                        child_root,
                        child_asid,
                        child_vspace_slots.weaken(),
                        child_vspace_ut.weaken(),
                        ProcessCodeImageConfig::ReadOnly,
                        user_image,
                        root_cnode,
                    )?;

                    let mut child_process = StandardProcess::new(
                        &mut child_vspace,
                        child_cnode,
                        mapped_region,
                        root_cnode,
                        proc_main as extern "C" fn(_) -> (),
                        params,
                        ut,
                        ut,
                        slots,
                        tpa,
                        Some(source),
                    )?;
                });
                child_process.start()?;

                match handler.await_message()? {
                    FaultOrMessage::Fault(_) => {
                        if c != &Command::ThrowFault {
                            panic!("Child process threw a fault when it should not have")
                        }
                    }
                    FaultOrMessage::Message(m) => match c {
                        Command::ThrowFault => {
                            return Err(TopLevelError::TestAssertionFailure(
                                "Command expected a fault to be thrown, not a message sent",
                            ))
                        }
                        Command::ReportTrue => {
                            if m != true {
                                return Err(TopLevelError::TestAssertionFailure(
                                    "Command expected success true to be reported",
                                ));
                            }
                        }
                        Command::ReportFalse => {
                            if m != false {
                                return Err(TopLevelError::TestAssertionFailure(
                                    "Command expected success false to be reported",
                                ));
                            }
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
