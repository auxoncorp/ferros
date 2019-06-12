use core::marker::PhantomData;
use core::ptr;

use crate::arch::cap::{page_state, AssignedASID, Page, UnassignedASID};
use crate::arch::{self, *};
use crate::bootstrap::UserImage;
use crate::cap::*;
use crate::userland::rights::CapRights;
use crate::vspace::*;

use selfe_sys::*;
use typenum::*;

pub(crate) use crate::arch::userland::process::*;
use crate::error::SeL4Error;

// TODO - consider renaming for clarity
pub trait RetypeForSetup: Sized + Send + Sync {
    type Output: Sized + Send + Sync;
}

pub type SetupVer<X> = <X as RetypeForSetup>::Output;

/// A helper zero-sized struct that forces structures
/// which have a field of its type to not auto-implement
/// core::marker::Send or core::marker::Sync.
///
/// Using this technique allows us to avoid a presently unstable
/// feature, `optin_builtin_traits` to explicitly opt-out of
/// implementing Send and Sync.
pub(crate) struct NeitherSendNorSync(PhantomData<*const ()>);

impl core::default::Default for NeitherSendNorSync {
    fn default() -> Self {
        NeitherSendNorSync(PhantomData)
    }
}

pub fn yield_forever() -> ! {
    unsafe {
        loop {
            seL4_Yield();
        }
    }
}

/// A TCB associated with a VSpace that has:
///  * A usable code image mapped/written into it
///  * Stack pages reserved and mapped
///  * Initial process state (e.g. parameter data) written into a seL4_UserContext
///  * Overflow initial process parameter struct data written into the stack
///  * Said seL4_UserContext written into the TCB
///  * An IPC buffer and CSpace and fault handler associated with that TCB
pub struct ReadyProcess {
    tcb: LocalCap<ThreadControlBlock>,
}

pub enum ProcessSetupError {
    ProcessParameterTooBigForStack,
    ProcessParameterHandoffSizeMismatch,
    VSpaceError(VSpaceError),
    SeL4Error(SeL4Error),
}

impl From<VSpaceError> for ProcessSetupError {
    fn from(e: VSpaceError) -> Self {
        ProcessSetupError::VSpaceError(e)
    }
}

impl From<SeL4Error> for ProcessSetupError {
    fn from(e: SeL4Error) -> Self {
        ProcessSetupError::SeL4Error(e)
    }
}

// TODO - Consider making this a parameter of ReadyProcess::new
pub type StackPageCount = U16;
pub type PrepareThreadCNodeSlots = U32;

impl ReadyProcess {
    pub fn new<'a, 'b, T: RetypeForSetup>(
        vspace: &mut VSpace,
        cspace: LocalCap<ChildCNode>,
        // TODO - provide a more limited view on the parent VSpace to avoid
        // accidental mutation and to reduce parameter confusion
        parent_vspace_scratch: &mut ScratchRegion<'a, 'b, crate::userland::process::StackPageCount>,
        function_descriptor: extern "C" fn(T) -> (),
        process_parameter: SetupVer<T>,
        stack_pages_ut: LocalCap<Untyped<U16>>,
        ipc_buffer_ut: LocalCap<Untyped<PageBits>>,
        tcb_ut: LocalCap<Untyped<<ThreadControlBlock as DirectRetype>::SizeBits>>,
        slots: LocalCNodeSlots<PrepareThreadCNodeSlots>,
        // TODO - CSpace and Fault Handler
        priority_authority: &LocalCap<ThreadPriorityAuthority>,
        fault_source: Option<crate::userland::FaultSource<role::Child>>,
    ) -> Result<Self, ProcessSetupError> {
        // TODO - lift these checks to compile-time, as static assertions
        // Note - This comparison is conservative because technically
        // we can fit some of the params into available registers.
        if core::mem::size_of::<SetupVer<T>>() > (StackPageCount::USIZE * arch::PageBytes::USIZE) {
            return Err(ProcessSetupError::ProcessParameterTooBigForStack);
        }
        if core::mem::size_of::<SetupVer<T>>() != core::mem::size_of::<T>() {
            return Err(ProcessSetupError::ProcessParameterHandoffSizeMismatch);
        }

        // Reserve a guard page before the stack
        vspace.skip_pages(1)?;

        // Carve off the stack_pages
        let (stack_slots, slots): (LocalCNodeSlots<StackPageCount>, _) = slots.alloc();
        //let stack_pages: CapRange<UnmappedPage<memory_kind::General>, role::Local, StackPageCount> =
        //    stack_pages_ut.retype_multi(stack_slots)?;
        let mut stack_pages = UnmappedMemoryRegion::new(stack_pages_ut, stack_slots)?;

        // map the child stack into local memory so we can copy the contents
        // of the process params into it
        let (mut registers, param_size_on_stack) = parent_vspace_scratch.temporarily_map_region(
            &mut stack_pages,
            |mapped_region| unsafe {
                setup_initial_stack_and_regs(
                    &process_parameter as *const SetupVer<T> as *const usize,
                    core::mem::size_of::<SetupVer<T>>(),
                    (mapped_region.vaddr + mapped_region.size()) as *mut usize,
                )
            },
        )?;

        // Map the stack to the target address space
        let mapped_stack_pages = vspace.map_region(stack_pages, CapRights::R)?; // TODO - revisit - do we need RW?
        let stack_pointer =
            mapped_stack_pages.vaddr + mapped_stack_pages.size() - param_size_on_stack;

        registers.sp = stack_pointer;
        registers.pc = function_descriptor as usize;

        // TODO - Probably ought to suspend or destroy the thread instead of endlessly yielding
        set_thread_link_register(&mut registers, yield_forever);

        // Reserve a guard page after the stack
        vspace.skip_pages(1)?;

        // Allocate and map the ipc buffer
        let (ipc_slots, slots) = slots.alloc();
        let ipc_buffer = ipc_buffer_ut.retype(ipc_slots)?;
        let ipc_buffer = vspace.map_given_page(ipc_buffer, CapRights::RW)?; // TODO - revisit - do we need RW?

        //// allocate the thread control block
        let (tcb_slots, slots) = slots.alloc();
        let mut tcb = tcb_ut.retype(tcb_slots)?;

        tcb.configure(cspace, fault_source, &vspace, ipc_buffer)?;
        unsafe {
            let err = seL4_TCB_WriteRegisters(
                tcb.cptr,
                0,
                0,
                // all the regs
                core::mem::size_of::<seL4_UserContext>() / core::mem::size_of::<usize>(),
                &mut registers,
            );
            if err != 0 {
                return Err(ProcessSetupError::SeL4Error(SeL4Error::TCBWriteRegisters(
                    err,
                )));
            }

            // TODO - priority management could be exposed once we plan on actually using it
            let err = seL4_TCB_SetPriority(tcb.cptr, priority_authority.cptr, 255);
            if err != 0 {
                return Err(ProcessSetupError::SeL4Error(SeL4Error::TCBSetPriority(err)));
            }
        }
        Ok(ReadyProcess { tcb })
    }

    pub fn start(self) -> Result<(), SeL4Error> {
        unsafe {
            let err = seL4_TCB_Resume(self.tcb.cptr);
            if err != 0 {
                return Err(SeL4Error::TCBResume(err));
            }
            Ok(())
        }
    }
}
