//! A test verifying that, should a process need a writable copy of
//! the user image, that such a write cannot affect another process'
//! copy of the user image.
use core::ptr;

use ferros::alloc::{smart_alloc, ut_buddy};
use ferros::bootstrap::UserImage;
use ferros::cap::*;
use ferros::userland::{
    call_channel, fault_or_message_channel, Caller, FaultOrMessage, ReadyProcess, Responder,
    RetypeForSetup, Sender,
};
use ferros::vspace::{ProcessCodeImageConfig, ScratchRegion, VSpace};

use typenum::*;

use super::TopLevelError;

type U42768 = Sum<U32768, U10000>;

#[ferros_test::ferros_test]
pub fn dont_tread_on_me(
    local_slots: LocalCNodeSlots<U42768>,
    local_ut: LocalCap<Untyped<U27>>,
    asid_pool: LocalCap<ASIDPool<U2>>,
    local_vspace_scratch: &mut ScratchRegion,
    root_cnode: &LocalCap<LocalCNode>,
    user_image: &UserImage<role::Local>,
    tpa: &LocalCap<ThreadPriorityAuthority>,
) -> Result<(), TopLevelError> {
    let uts = ut_buddy(local_ut);

    smart_alloc!(|slots: local_slots, ut: uts| {
        let (proc1_cspace, proc1_slots) = retype_cnode::<U8>(ut, slots)?;
        let (proc2_cspace, proc2_slots) = retype_cnode::<U8>(ut, slots)?;
    });
    smart_alloc!(|slots: local_slots, ut: uts| {
        let (proc1_asid, asid_pool) = asid_pool.alloc();
        let (proc2_asid, _asid_pool) = asid_pool.alloc();

        let proc1_root = retype(ut, slots)?;
        let proc1_vspace_slots: LocalCNodeSlots<U256> = slots;
        let proc1_vspace_ut: LocalCap<Untyped<U12>> = ut;

        let proc1_vspace = VSpace::new(
            proc1_root,
            proc1_asid,
            proc1_vspace_slots.weaken(),
            proc1_vspace_ut.weaken(),
            ProcessCodeImageConfig::ReadOnly,
            user_image,
            root_cnode,
        )?;

        let proc2_root = retype(ut, slots)?;
        let proc2_vspace_slots: LocalCNodeSlots<U256> = slots;
        let proc2_vspace_ut: LocalCap<Untyped<U12>> = ut;

        // this is not going to work until we can infer the type for ut.
        let proc2_vspace = VSpace::new(
            proc2_root,
            proc2_asid,
            proc2_vspace_slots.weaken(),
            proc2_vspace_ut.weaken(),
            ProcessCodeImageConfig::ReadWritable {
                untyped: ut.weaken(),
            },
            user_image,
            root_cnode,
        )?;

        let (slots1, proc1_slots) = proc1_slots.alloc();
        let (ipc_setup, responder) = call_channel(ut, &root_cnode, slots, slots1)?;
        let (proc1_outcome_sender_slot, _proc1_slots) = proc1_slots.alloc();
        let (fault_source, outcome_sender, handler) =
            fault_or_message_channel(&root_cnode, ut, slots, proc1_outcome_sender_slot, slots)?;

        let (slots2, _) = proc2_slots.alloc();
        let caller = ipc_setup.create_caller(slots2)?;

        let proc1_params = proc1::Proc1Params {
            rspdr: responder,
            outcome_sender,
        };
        let proc2_params = proc2::Proc2Params { cllr: caller };

        let proc1_process = ReadyProcess::new(
            &mut proc1_vspace,
            proc1_cspace,
            local_vspace_scratch,
            proc1::run,
            proc1_params,
            ut,
            ut,
            ut,
            slots,
            tpa,
            None, // fault
        )?;
        proc1_process.start()?;

        let proc2_process = ReadyProcess::new(
            &mut proc2_vspace,
            proc2_cspace,
            local_vspace_scratch,
            proc2::run,
            proc2_params,
            ut,
            ut,
            ut,
            slots,
            tpa,
            None, // fault
        )?;
        proc2_process.start()?;
    });

    match handler.await_message()? {
        FaultOrMessage::Message(true) if to_be_changed() => Ok(()),
        _ => Err(TopLevelError::TestAssertionFailure(
            "Child process should have reported success",
        )),
    }
}

/// The function that the proc2 child process will attempt to mutate
#[allow(dead_code)]
fn to_be_changed() -> bool {
    true
}

/// The substitue function that proc2 will attempt to put in the place of `to_be_changed`
#[allow(dead_code)]
fn substitute() -> bool {
    false
}

pub mod proc1 {
    use super::*;

    pub struct Proc1Params<Role: CNodeRole> {
        pub rspdr: Responder<(), (), Role>,
        pub outcome_sender: Sender<bool, Role>,
    }

    impl RetypeForSetup for Proc1Params<role::Local> {
        type Output = Proc1Params<role::Child>;
    }

    pub extern "C" fn run(params: Proc1Params<role::Local>) {
        let Proc1Params {
            rspdr,
            outcome_sender,
        } = params;
        rspdr
            .reply_recv(|_| {
                outcome_sender
                    .blocking_send(&to_be_changed())
                    .expect("failed to send test outcome");
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
        // Change the to_be_changed function to point to something different
        unsafe {
            let tbc_ptr = to_be_changed as *mut usize;
            ptr::write_volatile(tbc_ptr, substitute as usize);
        }
        params
            .cllr
            .blocking_call(&())
            .expect("blocking call blew up");
    }
}
