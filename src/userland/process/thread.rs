use crate::arch::*;
use crate::cap::*;
use crate::pow::{Pow, _Pow};
use crate::vspace::*;
use core::ops::Sub;

use selfe_sys::*;
use typenum::*;

use crate::error::{ErrorExt, SeL4Error};

use super::*;

/// A thread in Ferros is a TCB associated with a parent VSpace
/// that has:
///  * A usable code image mapped/written into it.
///  * A mapped stack.
///  * Initial process state (e.g. parameter data) written into a
///    `seL4_UserContext` and/or its stack.
///  * Said seL4_UserContext written into the TCB.
///  * An IPC buffer and CSpace and fault handler associated with that
///    TCB.
pub struct Thread<StackBitSize: Unsigned = DefaultStackBitSize> {
    tcb: LocalCap<ThreadControlBlock>,
    _stack_bit_size: PhantomData<StackBitSize>,
}

impl<StackBitSize: Unsigned> Thread<StackBitSize> {
    pub fn new<T: RetypeForSetup>(
        virtual_address_space_root: &LocalCap<crate::arch::PagingRoot>,
        cspace: LocalCap<ChildCNode>,
        stack_region: MappedMemoryRegion<StackBitSize, shared_status::Exclusive>,
        function_descriptor: extern "C" fn(T) -> (),
        process_parameter: SetupVer<T>,
        ipc_buffer: MappedMemoryRegion<PageBits, shared_status::Exclusive>,
        tcb_ut: LocalCap<Untyped<<ThreadControlBlock as DirectRetype>::SizeBits>>,
        slots: LocalCNodeSlots<U1>,
        priority_authority: &LocalCap<ThreadPriorityAuthority>,
        fault_source: Option<crate::userland::FaultSource<role::Child>>,
    ) -> Result<Thread<StackBitSize>, ThreadSetupError>
    where
        StackBitSize: IsGreaterOrEqual<PageBits>,
        StackBitSize: Sub<PageBits>,
        <StackBitSize as Sub<PageBits>>::Output: Unsigned,
        <StackBitSize as Sub<PageBits>>::Output: _Pow,
        Pow<<StackBitSize as Sub<PageBits>>::Output>: Unsigned,
    {
        if ipc_buffer.asid() != stack_region.asid() {
            return Err(ThreadSetupError::StackRegionASIDMustMatchIPCBufferASID);
        }
        // TODO - lift these checks to compile-time, as static assertions
        // Note - This comparison is conservative because technically
        // we can fit some of the params into available registers.
        if core::mem::size_of::<SetupVer<T>>() > 2usize.pow(StackBitSize::U32) {
            return Err(ThreadSetupError::ThreadParameterTooBigForStack);
        }
        if core::mem::size_of::<SetupVer<T>>() != core::mem::size_of::<T>() {
            return Err(ThreadSetupError::ThreadParameterHandoffSizeMismatch);
        }

        // Map the stack to the target address space
        let stack_top = stack_region.vaddr() + stack_region.size_bytes();
        let mapped_stack_pages = stack_region;

        // map the child stack into local memory so we can copy the contents
        // of the process params into it
        let (mut registers, param_size_on_stack) = unsafe {
            setup_initial_stack_and_regs(
                &process_parameter as *const SetupVer<T> as *const usize,
                core::mem::size_of::<SetupVer<T>>(),
                stack_top as *mut usize,
                mapped_stack_pages.vaddr() + mapped_stack_pages.size_bytes(),
            )
        };

        let stack_pointer =
            mapped_stack_pages.vaddr() + mapped_stack_pages.size_bytes() - param_size_on_stack;

        registers.sp = stack_pointer;
        registers.pc = function_descriptor as usize;

        // TODO - Probably ought to suspend or destroy the thread instead of endlessly yielding
        set_thread_link_register(&mut registers, yield_forever);

        //// allocate the thread control block
        let (tcb_slots, _slots) = slots.alloc();
        let mut tcb = tcb_ut.retype(tcb_slots)?;

        tcb.configure(
            cspace,
            fault_source,
            virtual_address_space_root,
            Some(ipc_buffer.to_page()),
        )?;
        unsafe {
            seL4_TCB_WriteRegisters(
                tcb.cptr,
                0,
                0,
                // all the regs
                core::mem::size_of::<seL4_UserContext>() / core::mem::size_of::<usize>(),
                &mut registers,
            )
            .as_result()
            .map_err(|e| ThreadSetupError::SeL4Error(SeL4Error::TCBWriteRegisters(e)))?;

            // TODO - priority management could be exposed once we
            // plan on actually using it
            tcb.set_priority(priority_authority, 255)?;
        }
        Ok(Thread {
            tcb,
            _stack_bit_size: PhantomData,
        })
    }

    pub fn start(self) -> Result<(), SeL4Error> {
        unsafe { seL4_TCB_Resume(self.tcb.cptr) }
            .as_result()
            .map_err(|e| SeL4Error::TCBResume(e))
    }
}
#[derive(Debug)]
pub enum ThreadSetupError {
    ThreadParameterTooBigForStack,
    ThreadParameterHandoffSizeMismatch,
    StackRegionASIDMustMatchIPCBufferASID,
    SeL4Error(SeL4Error),
}

impl From<SeL4Error> for ThreadSetupError {
    fn from(e: SeL4Error) -> Self {
        ThreadSetupError::SeL4Error(e)
    }
}
