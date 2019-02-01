use core::marker::PhantomData;
use core::mem;
use core::ops::Sub;

use cross_queue::{ArrayQueue, Slot};

use crate::userland::double_door::{QPtrType, QueueHandle};
use crate::userland::paging::PageBytes;
use crate::userland::{
    role, AssignedPageDirectory, Badge, CNodeRole, Cap, CapRights, ChildCNode, IPCError,
    LocalCNode, LocalCap, MappedPageTable, Notification, UnmappedPage, Untyped, VSpace,
};
use generic_array::ArrayLength;
use sel4_sys::{seL4_Signal, seL4_Wait};
use typenum::operator_aliases::{Diff, Sub1};
use typenum::{IsGreater, Unsigned, B1, U0, U1, U12, U2, U4, U5};

pub mod queue {
    use super::*;
    use typenum::type_operators::Cmp;
    use typenum::Greater;
    use typenum::UTerm;

    enum QueueError {
        Bad,
    }

    // Per Consumer: Create a new Notification associate with a type
    // managing badge-bit capacity one copy of the capability to that
    // notification in the CSpace of the consumer thread (with read
    // permissions) one path for dealing with the "interrupt" based
    // wakeup
    //
    // Per Queue: backing shared page(s) per queue a single bit index
    // from the badge bit-space per queue an associated element type
    // access to the local (parent?) VSpace in order to do local
    // mapping for setup? OR we do this in consumer
    //
    // Per Producer: a copy of the notification capability with write
    // permissions in the CSpace of the producer thread a mapping of
    // the queue backing pages for the relevant queue
    //
    // pub fn setup_consumer() -> Consumer

    pub struct Consumer2<
        Role: CNodeRole,
        E,
        ESize: Unsigned,
        EP: QPtrType<E, ESize>,
        F,
        FSize: Unsigned,
        FP: QPtrType<F, FSize>,
    >
    where
        ESize: IsGreater<U0>,
        ESize: ArrayLength<Slot<E>>,
        ESize: IsGreater<U0>,
        ESize: Cmp<U0, Output = Greater>,
        FSize: IsGreater<U0>,
        FSize: ArrayLength<Slot<F>>,
        FSize: IsGreater<U0>,
        FSize: Cmp<U0, Output = Greater>,
    {
        notification: Cap<Notification, Role>,
        queues: (
            QueueHandle<E, Role, ESize, EP>,
            QueueHandle<F, Role, FSize, FP>,
        ),
    }

    impl<
            Role: CNodeRole,
            E,
            ESize: Unsigned,
            EP: QPtrType<E, ESize>,
            F,
            FSize: Unsigned,
            FP: QPtrType<F, FSize>,
        > Consumer2<Role, E, ESize, EP, F, FSize, FP>
    where
        ESize: IsGreater<U0>,
        ESize: ArrayLength<Slot<E>>,
        ESize: IsGreater<U0>,
        ESize: Cmp<U0, Output = Greater>,
        FSize: IsGreater<U0>,
        FSize: ArrayLength<Slot<F>>,
        FSize: IsGreater<U0>,
        FSize: Cmp<U0, Output = Greater>,
    {
        fn new(
            ntf: Cap<Notification, Role>,
            qh1: QueueHandle<E, Role, ESize, EP>,
            qh2: QueueHandle<F, Role, FSize, FP>,
        ) -> Self {
            Self {
                notification: ntf,
                queues: (qh1, qh2),
            }
        }
    }

    pub struct Consumer3<
        Role: CNodeRole,
        E,
        ESize: Unsigned,
        EP: QPtrType<E, ESize>,
        F,
        FSize: Unsigned,
        FP: QPtrType<F, FSize>,
        G,
        GSize: Unsigned,
        GP: QPtrType<G, GSize>,
    >
    where
        ESize: IsGreater<U0>,
        ESize: ArrayLength<Slot<E>>,
        ESize: IsGreater<U0>,
        ESize: Cmp<U0, Output = Greater>,
        FSize: IsGreater<U0>,
        FSize: ArrayLength<Slot<F>>,
        FSize: IsGreater<U0>,
        FSize: Cmp<U0, Output = Greater>,
        GSize: IsGreater<U0>,
        GSize: ArrayLength<Slot<G>>,
        GSize: IsGreater<U0>,
        GSize: Cmp<U0, Output = Greater>,
    {
        notification: Cap<Notification, Role>,
        queues: (
            QueueHandle<E, Role, ESize, EP>,
            QueueHandle<F, Role, FSize, FP>,
            QueueHandle<G, Role, GSize, GP>,
        ),
    }

    impl<
            Role: CNodeRole,
            E,
            ESize: Unsigned,
            EP: QPtrType<E, ESize>,
            F,
            FSize: Unsigned,
            FP: QPtrType<F, FSize>,
            G,
            GSize: Unsigned,
            GP: QPtrType<G, GSize>,
        > Consumer3<Role, E, ESize, EP, F, FSize, FP, G, GSize, GP>
    where
        ESize: IsGreater<U0>,
        ESize: ArrayLength<Slot<E>>,
        ESize: IsGreater<U0>,
        ESize: Cmp<U0, Output = Greater>,
        FSize: IsGreater<U0>,
        FSize: ArrayLength<Slot<F>>,
        FSize: IsGreater<U0>,
        FSize: Cmp<U0, Output = Greater>,
        GSize: IsGreater<U0>,
        GSize: ArrayLength<Slot<G>>,
        GSize: IsGreater<U0>,
        GSize: Cmp<U0, Output = Greater>,
    {
        fn new(
            ntf: Cap<Notification, Role>,
            qh1: QueueHandle<E, Role, ESize, EP>,
            qh2: QueueHandle<F, Role, FSize, FP>,
            qh3: QueueHandle<G, Role, GSize, GP>,
        ) -> Self {
            Self {
                notification: ntf,
                queues: (qh1, qh2, qh3),
            }
        }
    }

}

pub mod sync {
    use super::*;
    /// A synchronous call channel backed by a page of shared memory
    pub fn extended_call_channel<
        ScratchFreeSlots: Unsigned,
        CallerFreeSlots: Unsigned,
        ResponderFreeSlots: Unsigned,
        CallerPageDirFreeSlots: Unsigned,
        CallerPageTableFreeSlots: Unsigned,
        CallerFilledPageTableCount: Unsigned,
        ResponderPageDirFreeSlots: Unsigned,
        ResponderPageTableFreeSlots: Unsigned,
        ResponderFilledPageTableCount: Unsigned,
        Req: Send + Sync,
        Rsp: Send + Sync,
    >(
        local_cnode: LocalCap<LocalCNode<ScratchFreeSlots>>,
        shared_page_ut: LocalCap<Untyped<U12>>,
        call_notification_ut: LocalCap<Untyped<U4>>,
        response_notification_ut: LocalCap<Untyped<U4>>,
        caller_vspace: VSpace<
            CallerPageDirFreeSlots,
            CallerPageTableFreeSlots,
            CallerFilledPageTableCount,
            role::Child,
        >,
        responder_vspace: VSpace<
            ResponderPageDirFreeSlots,
            ResponderPageTableFreeSlots,
            ResponderFilledPageTableCount,
            role::Child,
        >,
        child_cnode_caller: LocalCap<ChildCNode<CallerFreeSlots>>,
        child_cnode_responder: LocalCap<ChildCNode<ResponderFreeSlots>>,
    ) -> Result<
        (
            LocalCap<ChildCNode<Diff<CallerFreeSlots, U2>>>,
            LocalCap<ChildCNode<Diff<ResponderFreeSlots, U2>>>,
            ExtendedCaller<Req, Rsp, role::Child>,
            ExtendedResponder<Req, Rsp, role::Child>,
            VSpace<
                CallerPageDirFreeSlots,
                Sub1<CallerPageTableFreeSlots>,
                CallerFilledPageTableCount,
                role::Child,
            >,
            VSpace<
                ResponderPageDirFreeSlots,
                Sub1<ResponderPageTableFreeSlots>,
                ResponderFilledPageTableCount,
                role::Child,
            >,
            LocalCap<LocalCNode<Diff<ScratchFreeSlots, U5>>>,
        ),
        IPCError,
    >
    where
        ScratchFreeSlots: Sub<U5>,
        Diff<ScratchFreeSlots, U5>: Unsigned,

        CallerPageTableFreeSlots: Sub<B1>,
        Sub1<CallerPageTableFreeSlots>: Unsigned,

        ResponderPageTableFreeSlots: Sub<B1>,
        Sub1<ResponderPageTableFreeSlots>: Unsigned,

        CallerFreeSlots: Sub<U2>,
        Diff<CallerFreeSlots, U2>: Unsigned,

        ResponderFreeSlots: Sub<U2>,
        Diff<ResponderFreeSlots, U2>: Unsigned,

        CallerFilledPageTableCount: ArrayLength<LocalCap<MappedPageTable<U0, role::Child>>>,
        ResponderFilledPageTableCount: ArrayLength<LocalCap<MappedPageTable<U0, role::Child>>>,
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

        let (local_cnode, remainder_local_cnode) = local_cnode.reserve_region::<U5>();
        let (child_cnode_caller, remainder_child_cnode_caller) =
            child_cnode_caller.reserve_region::<U2>();
        let (child_cnode_responder, remainder_child_cnode_responder) =
            child_cnode_responder.reserve_region::<U2>();

        let (shared_page, local_cnode) =
            shared_page_ut.retype_local::<_, UnmappedPage>(local_cnode)?;

        let (caller_shared_page, local_cnode) =
            shared_page.copy_inside_cnode(local_cnode, CapRights::RW)?;
        let (caller_shared_page, caller_vspace) = caller_vspace.map_page(caller_shared_page)?;

        let (responder_shared_page, local_cnode) =
            shared_page.copy_inside_cnode(local_cnode, CapRights::RW)?;
        let (responder_shared_page, responder_vspace) =
            responder_vspace.map_page(responder_shared_page)?;

        let (local_request_ready, local_cnode) =
            call_notification_ut.retype_local::<_, Notification>(local_cnode)?;
        let (local_response_ready, local_cnode) =
            response_notification_ut.retype_local::<_, Notification>(local_cnode)?;

        // -2 caller cnode slots
        let (caller_request_ready, child_cnode_caller) = local_request_ready.mint(
            &local_cnode,
            child_cnode_caller,
            CapRights::RWG,
            Badge::from(1 << 0),
        )?;
        let (caller_response_ready, _child_cnode_caller) = local_response_ready.mint(
            &local_cnode,
            child_cnode_caller,
            CapRights::RWG,
            Badge::from(1 << 1),
        )?;

        let caller = ExtendedCaller {
            inner: SyncExtendedIpcPair {
                request_ready: caller_request_ready,
                response_ready: caller_response_ready,
                shared_page_address: caller_shared_page.cap_data.vaddr,
                _req: PhantomData,
                _rsp: PhantomData,
                _role: PhantomData,
            },
        };

        let (responder_request_ready, child_cnode_responder) = local_request_ready.mint(
            &local_cnode,
            child_cnode_responder,
            CapRights::RWG,
            Badge::from(1 << 2),
        )?;
        let (responder_response_ready, _child_cnode_responder) = local_response_ready.mint(
            &local_cnode,
            child_cnode_responder,
            CapRights::RWG,
            Badge::from(1 << 3),
        )?;

        let responder = ExtendedResponder {
            inner: SyncExtendedIpcPair {
                request_ready: responder_request_ready,
                response_ready: responder_response_ready,
                shared_page_address: responder_shared_page.cap_data.vaddr,
                _req: PhantomData,
                _rsp: PhantomData,
                _role: PhantomData,
            },
        };
        Ok((
            remainder_child_cnode_caller,
            remainder_child_cnode_responder,
            caller,
            responder,
            caller_vspace,
            responder_vspace,
            remainder_local_cnode,
        ))
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
