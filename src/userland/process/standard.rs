use crate::arch::{self, *};
use crate::cap::*;
use crate::userland::rights::CapRights;
use crate::vspace::*;
use core::ops::{Add, Sub};

use selfe_sys::*;
use typenum::*;

use crate::error::{ErrorExt, SeL4Error};

use super::*;

/// A standard process in Ferros is a TCB associated with a VSpace
/// that has:
///  * A usable code image mapped/written into it.
///  * A mapped stack.
///  * Initial process state (e.g. parameter data) written into a
///    `seL4_UserContext` and/or its stack.
///  * Said seL4_UserContext written into the TCB.
///  * An IPC buffer and CSpace and fault handler associated with that
///    TCB.
pub struct StandardProcess<StackBitSize: Unsigned = DefaultStackBitSize> {
    tcb: LocalCap<ThreadControlBlock>,
    _stack_bit_size: PhantomData<StackBitSize>,
}

impl<StackBitSize: Unsigned> StandardProcess<StackBitSize> {
    pub fn new<T: RetypeForSetup>(
        vspace: &mut VSpace,
        cspace: LocalCap<ChildCNode>,
        parent_mapped_region: MappedMemoryRegion<StackBitSize, shared_status::Exclusive>,
        parent_cnode: &LocalCap<LocalCNode>,
        function_descriptor: extern "C" fn(T) -> (),
        process_parameter: SetupVer<T>,
        ipc_buffer_ut: LocalCap<Untyped<PageBits>>,
        tcb_ut: LocalCap<Untyped<<ThreadControlBlock as DirectRetype>::SizeBits>>,
        slots: LocalCNodeSlots<Sum<GranuleSlotCount<StackBitSize>, U2>>,
        priority_authority: &LocalCap<ThreadPriorityAuthority>,
        fault_source: Option<crate::userland::FaultSource<role::Child>>,
    ) -> Result<StandardProcess<StackBitSize>, ProcessSetupError>
    where
        GranuleSlotCount<StackBitSize>: Add<U2>,
        Sum<GranuleSlotCount<StackBitSize>, U2>: Unsigned,

        Sum<GranuleSlotCount<StackBitSize>, U2>: Sub<U2>,
        Diff<Sum<GranuleSlotCount<StackBitSize>, U2>, U2>: Unsigned,
        Diff<Sum<GranuleSlotCount<StackBitSize>, U2>, U2>:
            IsEqual<GranuleSlotCount<StackBitSize>, Output = True>,
    {
        if parent_mapped_region.asid() == vspace.asid() {
            return Err(
                ProcessSetupError::ParentMappedMemoryRegionASIDShouldNotMatchChildVSpaceASID,
            );
        }
        let (misc_slots, stack_slots) = slots.alloc::<U2>();
        // TODO - lift these checks to compile-time, as static assertions
        // Note - This comparison is conservative because technically
        // we can fit some of the params into available registers.
        if core::mem::size_of::<SetupVer<T>>() > 2usize.pow(StackBitSize::U32) {
            return Err(ProcessSetupError::ProcessParameterTooBigForStack);
        }
        if core::mem::size_of::<SetupVer<T>>() != core::mem::size_of::<T>() {
            return Err(ProcessSetupError::ProcessParameterHandoffSizeMismatch);
        }

        // Reserve a guard page before the stack
        vspace.skip_pages(1)?;

        // Map the stack to the target address space
        let stack_top = parent_mapped_region.vaddr() + parent_mapped_region.size_bytes();
        let (unmapped_stack_pages, _): (UnmappedMemoryRegion<StackBitSize, _>, _) =
            parent_mapped_region.share(stack_slots, parent_cnode, CapRights::RW)?;
        let mapped_stack_pages = vspace.map_shared_region_and_consume(
            unmapped_stack_pages,
            CapRights::RW,
            arch::vm_attributes::DEFAULT & arch::vm_attributes::EXECUTE_NEVER,
        )?;

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

        // Reserve a guard page after the stack
        vspace.skip_pages(1)?;

        // Allocate and map the ipc buffer
        let (ipc_slots, misc_slots) = misc_slots.alloc();
        let ipc_buffer = ipc_buffer_ut.retype(ipc_slots)?;
        let ipc_buffer = vspace.map_region(
            ipc_buffer.to_region(),
            CapRights::RW,
            arch::vm_attributes::DEFAULT & arch::vm_attributes::EXECUTE_NEVER,
        )?;

        // allocate the thread control block
        let (tcb_slots, _slots) = misc_slots.alloc();
        let mut tcb = tcb_ut.retype(tcb_slots)?;

        tcb.configure(cspace, fault_source, &vspace, Some(ipc_buffer.to_page()))?;
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
            .map_err(|e| ProcessSetupError::SeL4Error(SeL4Error::TCBWriteRegisters(e)))?;

            // TODO - priority management could be exposed once we
            // plan on actually using it
            tcb.set_priority(priority_authority, 255)?;
        }
        Ok(StandardProcess {
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
