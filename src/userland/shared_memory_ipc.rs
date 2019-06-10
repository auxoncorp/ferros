use core::marker::PhantomData;
use core::ops::Sub;

use selfe_sys::{seL4_Signal, seL4_Wait};
use typenum::operator_aliases::Sub1;
use typenum::{Unsigned, B1, U2, U5};

use crate::arch::cap::{page_state, Page};
use crate::arch::{PageBits, PageBytes};
use crate::cap::{
    role, Badge, CNodeRole, Cap, ChildCNodeSlots, DirectRetype, LocalCNode, LocalCNodeSlots,
    LocalCap, Notification, Untyped,
};
use crate::userland::{CapRights, IPCError};
use crate::vspace::VSpace;

pub mod sync {
    use super::*;
    /// A synchronous call channel backed by a page of shared memory
    pub fn extended_call_channel<
        CallerPageDirFreeSlots: Unsigned,
        CallerPageTableFreeSlots: Unsigned,
        ResponderPageDirFreeSlots: Unsigned,
        ResponderPageTableFreeSlots: Unsigned,
        Req: Send + Sync,
        Rsp: Send + Sync,
    >(
        local_cnode: &LocalCap<LocalCNode>,
        local_slots: LocalCNodeSlots<U5>,
        shared_page_ut: LocalCap<Untyped<PageBits>>,
        call_notification_ut: LocalCap<Untyped<<Notification as DirectRetype>::SizeBits>>,
        response_notification_ut: LocalCap<Untyped<<Notification as DirectRetype>::SizeBits>>,
        caller_vspace: &mut VSpace,
        responder_vspace: &mut VSpace,
        caller_slots: ChildCNodeSlots<U2>,
        responder_slots: ChildCNodeSlots<U2>,
    ) -> Result<
        (
            ExtendedCaller<Req, Rsp, role::Child>,
            ExtendedResponder<Req, Rsp, role::Child>,
        ),
        IPCError,
    >
    where
        CallerPageTableFreeSlots: Sub<B1>,
        Sub1<CallerPageTableFreeSlots>: Unsigned,

        ResponderPageTableFreeSlots: Sub<B1>,
        Sub1<ResponderPageTableFreeSlots>: Unsigned,
    {
        let request_size = core::mem::size_of::<Req>();
        let response_size = core::mem::size_of::<Rsp>();
        // TODO - Move this to compile-time somehow
        if request_size > PageBytes::USIZE {
            return Err(IPCError::RequestSizeTooBig);
        }
        if response_size > PageBytes::USIZE {
            return Err(IPCError::ResponseSizeTooBig);
        }

        let (slot, local_slots) = local_slots.alloc();
        let shared_page: LocalCap<Page<page_state::Unmapped>> = shared_page_ut.retype(slot)?;

        let (slot, local_slots) = local_slots.alloc();
        let caller_shared_page = shared_page.copy(&local_cnode, slot, CapRights::RW)?;
        let caller_shared_page = caller_vspace.map_given_page(caller_shared_page, CapRights::RW)?;

        let (slot, local_slots) = local_slots.alloc();
        let responder_shared_page = shared_page.copy(&local_cnode, slot, CapRights::RW)?;
        let responder_shared_page =
            responder_vspace.map_given_page(responder_shared_page, CapRights::RW)?;

        let (slot, local_slots) = local_slots.alloc();
        let local_request_ready: LocalCap<Notification> = call_notification_ut.retype(slot)?;

        let (slot, _local_slots) = local_slots.alloc();
        let local_response_ready: LocalCap<Notification> = response_notification_ut.retype(slot)?;

        let (caller_slot, caller_slots) = caller_slots.alloc();
        let caller_request_ready = local_request_ready.mint(
            &local_cnode,
            caller_slot,
            CapRights::RWG,
            Badge::from(1 << 0),
        )?;

        let (caller_slot, _caller_slots) = caller_slots.alloc();
        let caller_response_ready = local_response_ready.mint(
            &local_cnode,
            caller_slot,
            CapRights::RWG,
            Badge::from(1 << 1),
        )?;

        let caller = ExtendedCaller {
            inner: SyncExtendedIpcPair {
                request_ready: caller_request_ready,
                response_ready: caller_response_ready,
                shared_page_address: caller_shared_page.vaddr(),
                _req: PhantomData,
                _rsp: PhantomData,
                _role: PhantomData,
            },
        };

        let (responder_slot, responder_slots) = responder_slots.alloc();
        let responder_request_ready = local_request_ready.mint(
            &local_cnode,
            responder_slot,
            CapRights::RWG,
            Badge::from(1 << 2),
        )?;

        let (responder_slot, _responder_slots) = responder_slots.alloc();
        let responder_response_ready = local_response_ready.mint(
            &local_cnode,
            responder_slot,
            CapRights::RWG,
            Badge::from(1 << 3),
        )?;

        let responder = ExtendedResponder {
            inner: SyncExtendedIpcPair {
                request_ready: responder_request_ready,
                response_ready: responder_response_ready,
                shared_page_address: responder_shared_page.vaddr(),
                _req: PhantomData,
                _rsp: PhantomData,
                _role: PhantomData,
            },
        };
        Ok((caller, responder))
    }

    #[derive(Debug)]
    struct SyncExtendedIpcPair<Req: Sized, Rsp: Sized, Role: CNodeRole> {
        request_ready: Cap<Notification, Role>,
        response_ready: Cap<Notification, Role>,
        shared_page_address: usize,
        _req: PhantomData<Req>,
        _rsp: PhantomData<Rsp>,
        _role: PhantomData<Role>,
    }

    impl<Req: Sized, Rsp: Sized> SyncExtendedIpcPair<Req, Rsp, role::Local> {
        unsafe fn unchecked_copy_into_buffer<T: Sized>(&mut self, data: &T) {
            let shared: &mut T = core::mem::transmute(self.shared_page_address as *mut T);
            core::ptr::copy(data as *const T, shared as *mut T, 1);
        }
        unsafe fn unchecked_copy_from_buffer<T: Sized>(&self) -> T {
            let shared: &T = core::mem::transmute(self.shared_page_address as *const T);
            let mut data = core::mem::zeroed();
            core::ptr::copy_nonoverlapping(shared as *const T, &mut data as *mut T, 1);
            data
        }
    }

    #[derive(Debug)]
    pub struct ExtendedCaller<Req: Sized, Rsp: Sized, Role: CNodeRole> {
        inner: SyncExtendedIpcPair<Req, Rsp, Role>,
    }

    impl<Req, Rsp> ExtendedCaller<Req, Rsp, role::Local> {
        pub fn blocking_call<'a>(&mut self, request: &Req) -> Rsp {
            let mut sender_badge: usize = 0;
            unsafe {
                self.inner.unchecked_copy_into_buffer(request);
                seL4_Signal(self.inner.request_ready.cptr);
                seL4_Wait(
                    self.inner.response_ready.cptr,
                    &mut sender_badge as *mut usize,
                );
                self.inner.unchecked_copy_from_buffer()
            }
        }
    }

    #[derive(Debug)]
    pub struct ExtendedResponder<Req: Sized, Rsp: Sized, Role: CNodeRole> {
        inner: SyncExtendedIpcPair<Req, Rsp, Role>,
    }
    impl<Req, Rsp> ExtendedResponder<Req, Rsp, role::Local> {
        pub fn reply_recv<F>(self, f: F) -> !
        where
            F: Fn(&Req) -> (Rsp),
        {
            self.reply_recv_with_state((), move |req, state| (f(req), state))
        }

        pub fn reply_recv_with_state<F, State>(self, initial_state: State, f: F) -> !
        where
            F: Fn(&Req, State) -> (Rsp, State),
        {
            let mut inner = self.inner;
            let mut sender_badge: usize = 0;
            let mut response;
            let mut state = initial_state;
            loop {
                unsafe {
                    seL4_Wait(inner.request_ready.cptr, &mut sender_badge as *mut usize);
                    let out = f(&inner.unchecked_copy_from_buffer(), state);
                    response = out.0;
                    state = out.1;
                    inner.unchecked_copy_into_buffer(&response);
                    seL4_Signal(inner.response_ready.cptr);
                }
            }
        }
    }
}
