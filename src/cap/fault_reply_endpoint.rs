use crate::arch;
use crate::cap::{CNodeSlotsData, Cap, CapType, LocalCNodeSlot, LocalCap};
use crate::error::{ErrorExt, SeL4Error};
use core::marker::PhantomData;
use selfe_sys::*;
use selfe_wrap::error::{APIMethod, CNodeMethod};
use typenum::*;

#[derive(Debug)]
/// A FaultReply encapsulates a reply capability that goes with a fault; all you
/// can do is send an empty message to it, once, which resumes the fault source's
/// execution. After that, it destroys itself, and gives you back the cnode slot
/// where it was living. Or you can just destroy it, to get the cnode slot back.
pub struct FaultReplyEndpoint {
    original_slot_cptr: usize,
}

impl CapType for FaultReplyEndpoint {}

impl LocalCap<FaultReplyEndpoint> {
    /// Save the TCB reply capability into the given CNode slot. This expects to
    /// be Used only in response to a Fault.
    pub fn save_caller_and_create(
        slot: LocalCNodeSlot,
    ) -> Result<LocalCap<FaultReplyEndpoint>, SeL4Error> {
        let (cptr, offset, _) = slot.elim();

        unsafe {
            seL4_CNode_SaveCaller(
                cptr,               // _service
                offset,             // index
                arch::WordSize::U8, // depth
            )
        }
        .as_result()
        .map_err(|e| SeL4Error::new(APIMethod::CNode(CNodeMethod::SaveCaller), e))?;

        Ok(Cap {
            cptr: offset,
            _role: PhantomData,
            cap_data: FaultReplyEndpoint {
                original_slot_cptr: cptr,
            },
        })
    }

    fn to_slot(self) -> LocalCNodeSlot {
        Cap {
            cptr: self.cap_data.original_slot_cptr,
            _role: PhantomData,
            cap_data: CNodeSlotsData {
                offset: self.cptr,
                _role: PhantomData,
                _size: PhantomData,
            },
        }
    }

    /// Resume the thread that that caused the fault, consume the cap, and
    /// return the slot it was in.
    pub fn resume_faulted_thread(self) -> LocalCNodeSlot {
        let empty_msg = unsafe { seL4_MessageInfo_new(0, 0, 0, 0) };

        unsafe { seL4_Send(self.cptr, empty_msg) };

        // The manual says the kernel will 'invalidate' the reply cap after one
        // send, so we should be able to reuse its slot.
        self.to_slot()
    }
}
