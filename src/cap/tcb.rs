use selfe_sys::*;

use crate::cap::{
    granule_state, role, CapType, ChildCNode, CopyAliasable, DirectRetype, Granule, LocalCap,
    PhantomCap,
};
use crate::error::{ErrorExt, SeL4Error};
use crate::userland::FaultSource;
use crate::vspace::{self, VSpace};

#[derive(Debug)]
pub struct ThreadControlBlock {}

impl CapType for ThreadControlBlock {}

impl PhantomCap for ThreadControlBlock {
    fn phantom_instance() -> Self {
        Self {}
    }
}

impl DirectRetype for ThreadControlBlock {
    type SizeBits = crate::arch::TCBBits;
    fn sel4_type_id() -> usize {
        api_object_seL4_TCBObject as usize
    }
}

impl CopyAliasable for ThreadControlBlock {
    type CopyOutput = Self;
}
impl<'a> From<&'a ThreadControlBlock> for ThreadControlBlock {
    fn from(_val: &'a ThreadControlBlock) -> Self {
        PhantomCap::phantom_instance()
    }
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
impl<'a> From<&'a ThreadPriorityAuthority> for ThreadPriorityAuthority {
    fn from(_val: &'a ThreadPriorityAuthority) -> Self {
        PhantomCap::phantom_instance()
    }
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

    pub fn configure<VSpaceState: vspace::VSpaceState>(
        &mut self,
        cspace_root: LocalCap<ChildCNode>,
        fault_source: Option<FaultSource<role::Child>>,
        vspace: &VSpace<VSpaceState, role::Local>, // vspace_root,
        ipc_buffer: Option<LocalCap<Granule<granule_state::Mapped>>>,
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

        let (buffer_cap, buffer_vaddr) = if let Some(ipc_buffer) = ipc_buffer {
            (ipc_buffer.cptr, ipc_buffer.vaddr())
        } else {
            (seL4_CapNull as usize, 0)
        };

        unsafe {
            seL4_TCB_Configure(
                self.cptr,
                fault_source.map_or(seL4_CapNull as usize, |source| source.endpoint.cptr), // fault_ep.cptr,
                cspace_root.cptr,
                cspace_root_data,
                vspace.root_cptr(),
                seL4_NilData as usize, // vspace_root_data, always 0, reserved by kernel?
                buffer_vaddr,          // buffer address
                buffer_cap,            // bufferFrame capability
            )
        }
        .as_result()
        .map_err(|e| SeL4Error::TCBConfigure(e))
    }

    /// Set this TCB's priority.
    pub fn set_priority(
        &mut self,
        tpa: &LocalCap<ThreadPriorityAuthority>,
        prio: usize,
    ) -> Result<(), SeL4Error> {
        unsafe { seL4_TCB_SetPriority(self.cptr, tpa.cptr, prio) }
            .as_result()
            .map_err(|e| SeL4Error::TCBSetPriority(e))
    }
}
