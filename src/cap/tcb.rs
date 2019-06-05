use typenum::*;

use selfe_sys::*;

use crate::arch::cap::Page;
use crate::cap::{
    role, CNodeRole, CapType, ChildCNode, CopyAliasable, DirectRetype, LocalCap, PhantomCap,
};
use crate::error::SeL4Error;
use crate::userland::FaultSource;
use crate::vspace::VSpace;

#[derive(Debug)]
pub struct ThreadControlBlock {}

impl CapType for ThreadControlBlock {}

impl PhantomCap for ThreadControlBlock {
    fn phantom_instance() -> Self {
        Self {}
    }
}

impl DirectRetype for ThreadControlBlock {
    type SizeBits = U10;
    fn sel4_type_id() -> usize {
        api_object_seL4_TCBObject as usize
    }
}

impl CopyAliasable for ThreadControlBlock {
    type CopyOutput = Self;
}

/// A limited view on a ThreadControlBlock capability
/// that is only intended for use in establishing
/// the priority of child threads
#[derive(Debug)]
pub struct ThreadPriorityAuthority {}

impl CapType for ThreadPriorityAuthority {}

impl PhantomCap for ThreadPriorityAuthority {
    fn phantom_instance() -> Self {
        Self {}
    }
}

impl CopyAliasable for ThreadPriorityAuthority {
    type CopyOutput = Self;
}

impl AsRef<LocalCap<ThreadPriorityAuthority>> for LocalCap<ThreadControlBlock> {
    fn as_ref(&self) -> &LocalCap<ThreadPriorityAuthority> {
        unsafe { core::mem::transmute(self) }
    }
}

impl LocalCap<ThreadControlBlock> {
    pub fn downgrade_to_thread_priority_authority(self) -> LocalCap<ThreadPriorityAuthority> {
        unsafe { core::mem::transmute(self) }
    }

    pub(crate) fn configure<VSpaceRole: CNodeRole>(
        &mut self,
        cspace_root: LocalCap<ChildCNode>,
        fault_source: Option<FaultSource<role::Child>>,
        vspace_cptr: VSpace, // vspace_root,
        ipc_buffer: LocalCap<Page>,
    ) -> Result<(), SeL4Error> {
        // Set up the cspace's guard to take the part of the cptr that's not
        // used by the radix.
        let cspace_root_data = unsafe {
            seL4_CNode_CapData_new(
                0,                                                          // guard
                (seL4_WordBits - cspace_root.cap_data.radix as usize) as _, // guard size in bits
            )
        }
        .words[0] as usize;

        let tcb_err = unsafe {
            seL4_TCB_Configure(
                self.cptr,
                fault_source.map_or(seL4_CapNull as usize, |source| source.endpoint.cptr), // fault_ep.cptr,
                cspace_root.cptr,
                cspace_root_data,
                vspace_cptr.get_capability_pointer(), //vspace_root.cptr,
                seL4_NilData as usize, // vspace_root_data, always 0, reserved by kernel?
                ipc_buffer.cap_data.vaddr, // buffer address
                ipc_buffer.cptr,       // bufferFrame capability
            )
        };

        if tcb_err != 0 {
            Err(SeL4Error::TCBConfigure(tcb_err))
        } else {
            Ok(())
        }
    }
}
