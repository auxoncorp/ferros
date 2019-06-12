use core::marker::PhantomData;

use selfe_sys::*;

use crate::arch::fault::Fault;
use crate::cap::{
    role, Badge, CNodeRole, CNodeSlot, Cap, ChildCNodeSlot, DirectRetype, Endpoint, LocalCNode,
    LocalCNodeSlot, LocalCap, Untyped,
};
use crate::error::SeL4Error;
use crate::userland::{type_length_in_words, CapRights, IPCBuffer, IPCError, MessageInfo, Sender};

#[derive(Debug)]
pub enum FaultManagementError {
    SelfFaultHandlingForbidden,
    MessageSizeTooBig,
    SeL4Error(SeL4Error),
}

impl From<SeL4Error> for FaultManagementError {
    fn from(s: SeL4Error) -> Self {
        FaultManagementError::SeL4Error(s)
    }
}

pub struct FaultSinkSetup<SinkRole: CNodeRole> {
    // Local pointer to the endpoint, kept around for easy copying
    local_endpoint: LocalCap<Endpoint>,

    // Copy of the same endpoint, set up with the correct rights,
    // living in the CSpace of the CNode that will become
    // the root of the fault-handling process.
    sink_endpoint: Cap<Endpoint, SinkRole>,

    // To enable checking whether there is an accidental attempt
    // to wire up a process root CSpace as its own fault handler
    sink_cspace_local_cptr: usize,
}

impl<SinkRole: CNodeRole> FaultSinkSetup<SinkRole> {
    pub fn new(
        local_cnode: &LocalCap<LocalCNode>,
        untyped: LocalCap<Untyped<<Endpoint as DirectRetype>::SizeBits>>,
        endpoint_slot: LocalCNodeSlot,
        fault_sink_slot: CNodeSlot<SinkRole>,
    ) -> Result<Self, SeL4Error> {
        let sink_cspace_local_cptr = fault_sink_slot.cptr;

        let local_endpoint: LocalCap<Endpoint> = untyped.retype(endpoint_slot)?;

        let sink_endpoint = local_endpoint.copy(&local_cnode, fault_sink_slot, CapRights::RW)?;

        Ok(FaultSinkSetup {
            local_endpoint,
            sink_endpoint,
            sink_cspace_local_cptr,
        })
    }

    pub fn add_fault_source(
        &self,
        local_cnode: &LocalCap<LocalCNode>,
        fault_source_slot: ChildCNodeSlot,
        badge: Badge,
    ) -> Result<FaultSource<role::Child>, FaultManagementError> {
        if fault_source_slot.cptr == self.sink_cspace_local_cptr {
            return Err(FaultManagementError::SelfFaultHandlingForbidden);
        }

        let child_endpoint_fault_source =
            self.local_endpoint
                .mint_new(local_cnode, fault_source_slot, CapRights::RWG, badge)?;

        Ok(FaultSource {
            endpoint: child_endpoint_fault_source,
        })
    }

    pub fn sink(self) -> FaultSink<SinkRole> {
        FaultSink {
            endpoint: self.sink_endpoint,
        }
    }
}

/// Only supports establishing two child processes where one process will be watching for faults on the other.
/// Requires a separate input signature if we want the local/current thread to be the watcher due to
/// our consuming full instances of the local scratch CNode and the destination CNodes separately in this function.
pub fn setup_fault_endpoint_pair(
    local_cnode: &LocalCap<LocalCNode>,
    untyped: LocalCap<Untyped<<Endpoint as DirectRetype>::SizeBits>>,
    endpoint_slot: LocalCNodeSlot,
    fault_source_slot: ChildCNodeSlot,
    fault_sink_slot: ChildCNodeSlot,
) -> Result<(FaultSource<role::Child>, FaultSink<role::Child>), FaultManagementError> {
    let setup = FaultSinkSetup::new(&local_cnode, untyped, endpoint_slot, fault_sink_slot)?;
    let fault_source = setup.add_fault_source(&local_cnode, fault_source_slot, Badge::from(0))?;
    Ok((fault_source, setup.sink()))
}

/// The side of a fault endpoint that sends fault messages
#[derive(Debug)]
pub struct FaultSource<Role: CNodeRole> {
    pub(crate) endpoint: Cap<Endpoint, Role>,
}

/// The side of a fault endpoint that receives fault messages
#[derive(Debug)]
pub struct FaultSink<Role: CNodeRole> {
    pub(crate) endpoint: Cap<Endpoint, Role>,
}

impl FaultSink<role::Local> {
    pub fn wait_for_fault(&self) -> Fault {
        let mut sender: usize = 0;
        let info = unsafe { seL4_Recv(self.endpoint.cptr, &mut sender as *mut usize) }.into();
        (info, Badge::from(sender)).into()
    }
}

pub fn fault_or_message_channel<Msg: Sized, HandlerRole: CNodeRole>(
    local_cnode: &LocalCap<LocalCNode>,
    untyped: LocalCap<Untyped<<Endpoint as DirectRetype>::SizeBits>>,
    endpoint_slot: LocalCNodeSlot,
    fault_source_slot: ChildCNodeSlot,
    handler_slot: CNodeSlot<HandlerRole>,
) -> Result<
    (
        FaultSource<role::Child>,
        Sender<Msg, role::Child>,
        FaultOrMessageHandler<Msg, HandlerRole>,
    ),
    FaultManagementError,
> {
    if fault_source_slot.cptr == handler_slot.cptr {
        return Err(FaultManagementError::SelfFaultHandlingForbidden);
    }
    if core::mem::size_of::<Msg>() > IPCBuffer::<Msg, ()>::max_size() {
        return Err(FaultManagementError::MessageSizeTooBig);
    }

    // NB: This approach could be converted to use a `Setup` pattern to allow multiple fault-sources
    let local_endpoint: LocalCap<Endpoint> = untyped.retype(endpoint_slot)?;
    let handler_endpoint = local_endpoint.copy(&local_cnode, handler_slot, CapRights::RW)?;
    let child_endpoint_fault_source = local_endpoint.mint_new(
        local_cnode,
        fault_source_slot,
        CapRights::RWG,
        Badge::from(0),
    )?;

    Ok((
        FaultSource {
            // Alias the endpoint harmlessly because FaultSource exposes no public methods
            // and is intended only to be used to tell the kernel where to route faults
            // for the child thread's TCB
            endpoint: Cap {
                cptr: child_endpoint_fault_source.cptr,
                _role: PhantomData,
                cap_data: Endpoint {},
            },
        },
        Sender {
            endpoint: child_endpoint_fault_source,
            _msg: PhantomData,
        },
        FaultOrMessageHandler {
            endpoint: handler_endpoint,
            _msg: PhantomData,
        },
    ))
}

pub struct FaultOrMessageHandler<Msg: Sized, Role: CNodeRole> {
    endpoint: Cap<Endpoint, Role>,
    _msg: PhantomData<Msg>,
}

#[derive(Debug)]
pub enum FaultOrMessage<Msg: Sized> {
    Fault(Fault),
    Message(Msg),
}

impl<Msg: Sized> FaultOrMessageHandler<Msg, role::Local> {
    pub fn await_message(&self) -> Result<FaultOrMessage<Msg>, IPCError> {
        // Using unchecked_new is acceptable here because we check the message size
        // constraints during the construction of FaultOrMessageHandler
        let ipc_buffer: IPCBuffer<Msg, ()> = unsafe { IPCBuffer::unchecked_new() };
        let mut sender: usize = 0;
        // Do a regular receive to seed our initial value
        let msg_info: MessageInfo =
            unsafe { seL4_Recv(self.endpoint.cptr, &mut sender as *mut usize) }.into();

        let badge = Badge::from(sender);
        if msg_info.has_null_fault_label() {
            let msg_length_in_words = type_length_in_words::<Msg>();
            if msg_length_in_words != msg_info.length_words() {
                return Err(IPCError::RequestSizeMismatch);
            }
            Ok(FaultOrMessage::Message(ipc_buffer.copy_req_from_buffer()))
        } else {
            Ok(FaultOrMessage::Fault((msg_info, badge).into()))
        }
    }
}
