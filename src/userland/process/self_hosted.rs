use typenum::*;

use crate::arch::{self, PageBits};
use crate::cap::{
    role, ChildCNode, DirectRetype, LocalCNode, LocalCNodeSlots, LocalCap, ThreadControlBlock,
    ThreadPriorityAuthority, Untyped,
};
use crate::vspace::*;

use super::*;

pub struct SelfHostedProcess {
    tcb: LocalCap<ThreadControlBlock>,
}

struct SelfHostedParams<T: RetypeForSetup> {
    params: T,
    vspace: VSpace,
    child_main: extern "C" fn(VSpace, T) -> (),
}

impl RetypeForSetup for SelfHostedParams<role::Local> {
    type Output = SelfHostedParams<role::Child>;
}

fn self_hosted_run<T: RetypeForSetup>(sh_params: SelfHostedParams<T>) {
    let SelfHostedParams {
        params,
        vspace,
        child_main,
    } = sh_params;
    child_main(vspace, params);
}

impl SelfHostedProcess {
    pub fn new<T: RetypeForSetup>(
        child_vspace: VSpace,
        cspace: LocalCap<ChildCNode>,
        parent_mapped_region: MappedMemoryRegion<StackPageCount, shared_status::Exclusive>,
        parent_cnode: &LocalCap<LocalCNode>,
        function_descriptor: extern "C" fn(VSpace, T) -> (),
        process_parameter: SetupVer<T>,
        ipc_buffer_ut: LocalCap<Untyped<PageBits>>,
        tcb_ut: LocalCap<Untyped<<ThreadControlBlock as DirectRetype>::SizeBits>>,
        slots: LocalCNodeSlots<PrepareThreadCNodeSlots>,
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

        // Allocate and map the ipc buffer
        let (ipc_slots, slots) = slots.alloc();
        let ipc_buffer = ipc_buffer_ut.retype(ipc_slots)?;
        let ipc_buffer = child_vspace.map_given_page(ipc_buffer, CapRights::RW)?;

        // allocate the thread control block
        let (tcb_slots, _slots) = slots.alloc();
        let mut tcb = tcb_ut.retype(tcb_slots)?;

        tcb.configure(cspace, fault_source, &child_vspace, ipc_buffer)?;

        // Reserve a guard page before the stack
        child_vspace.skip_pages(1)?;

        // Map the stack to the target address space
        let stack_top = parent_mapped_region.vaddr() + parent_mapped_region.size();
        let (page_slots, slots) = slots.alloc();
        let (unmapped_stack_pages, _) =
            parent_mapped_region.share(page_slots, parent_cnode, CapRights::RW)?;
        let mapped_stack_pages =
            child_vspace.map_shared_region_and_consume(unmapped_stack_pages, CapRights::RW)?;

        child_vspace.skip_pages(1)?;

        let sh_params = SelfHostedParams {
            vspace: child_vspace,
            params: process_parameter,
            child_main: function_descriptor,
        };

        // map the child stack into local memory so we can copy the contents
        // of the process params into it
        let (mut registers, param_size_on_stack) = unsafe {
            setup_initial_stack_and_regs(
                &sh_params as *const SetupVer<T> as *const usize,
                core::mem::size_of::<SetupVer<T>>(),
                stack_top as *mut usize,
                mapped_stack_pages.vaddr() + mapped_stack_pages.size(),
            )
        };

        let stack_pointer =
            mapped_stack_pages.vaddr() + mapped_stack_pages.size() - param_size_on_stack;

        registers.sp = stack_pointer;
        registers.pc = self_hosted_run as usize;

        // TODO - Probably ought to suspend or destroy the thread instead of endlessly yielding
        set_thread_link_register(&mut registers, yield_forever);

        // Reserve a guard page after the stack

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

            // TODO - priority management could be exposed once we plan on actually using it
            seL4_TCB_SetPriority(tcb.cptr, priority_authority.cptr, 255)
                .as_result()
                .map_err(|e| ProcessSetupError::SeL4Error(SeL4Error::TCBSetPriority(e)))?;
        }
        Ok(SelfHostedProcess { tcb })
    }
}
