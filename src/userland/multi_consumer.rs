//! A pattern for async IPC with driver processes/threads where there
//! is a single (driver) consumer thread that is waiting on a single
//! notification.  There are two possible badge values for the
//! notification, and based on the badge, the consumer will do one of
//! the following:
//!
//! A) Execute a custom, interrupt-handling-specialized path.
//! B) Attempt to read from a shared memory queue. If an element is
//! found, process it.
//!
//! The alpha-path is intended to be bound to an interrupt
//! notification, but technically will work out of the box with any
//! regular notification-sender badged to match the A) path.
//!
//! There may be many other threads producing to the shared memory
//! queue. A queue-producer thread requires:
//! * A capability to the notification, badged to correspond to the
//! queue-path.
//! * The memory region where the queue lives mapped into its VSpace.
//! * A pointer to the shared memory queue valid in its VSpace.
//!
//! There are two doors into the consumer thread. Do you pick door A
//! or B?
//!
//! let (irq_consumer, consumer_token) = InterruptConsumer::new(
//!     notification_ut,
//!     irq_control,
//!     local_cnode,
//!     local_slots,
//!     consumer_slots)?;
//! let (consumer1, producer_setup) = irq_consumer.add_queue(
//!     consumer_token,
//!     shared_region_ut,
//!     local_vspace,
//!     consumer_vspace,
//!     local_cnode,
//!     dest_slots)?;
use core::marker::PhantomData;

use cross_queue::{ArrayQueue, PushError, Slot};

use generic_array::ArrayLength;

use selfe_sys::{seL4_Signal, seL4_Wait};

use typenum::*;

use crate::arch::{self, PageBits, PageBytes};
use crate::cap::{
    irq_state, role, Badge, CNodeRole, Cap, ChildCNodeSlot, ChildCNodeSlots, DirectRetype,
    IRQControl, IRQError, IRQHandler, InternalASID, LocalCNode, LocalCNodeSlot, LocalCNodeSlots,
    LocalCap, MaxIRQCount, Notification, PhantomCap, RetypeError, Untyped,
};
use crate::error::SeL4Error;
use crate::userland::CapRights;
use crate::vspace::{
    shared_status, MappedMemoryRegion, ScratchRegion, UnmappedMemoryRegion, VSpace, VSpaceError,
};

/// A multi-consumer that consumes interrupt-style notifications
///
/// Designed to be handed to a new process as a member of the
/// initial thread parameters struct (see `VSpace::prepare_thread`).
pub struct InterruptConsumer<IRQ: Unsigned, Role: CNodeRole>
where
    IRQ: IsLess<MaxIRQCount, Output = True>,
{
    irq_handler: Cap<IRQHandler<IRQ, irq_state::Set>, Role>,
    interrupt_badge: Badge,
    notification: Cap<Notification, Role>,
}

/// A multi-consumer that consumes interrupt-style notifications and from 1 queue
///
/// Designed to be handed to a new process as a member of the
/// initial thread parameters struct (see `VSpace::prepare_thread`).
pub struct Consumer1<Role: CNodeRole, T: Sized + Sync + Send, QLen: Unsigned, IRQ: Unsigned = U0>
where
    IRQ: IsLess<MaxIRQCount, Output = True>,
    QLen: IsGreater<U0, Output = True>,
    QLen: ArrayLength<Slot<T>>,
{
    irq_handler: Option<Cap<IRQHandler<IRQ, irq_state::Set>, Role>>,
    interrupt_badge: Badge,
    notification: Cap<Notification, Role>,
    queue_badge: Badge,
    queue: QueueHandle<T, Role, QLen>,
}

/// A multi-consumer that consumes interrupt-style notifications and from 2 queues
///
/// Designed to be handed to a new process as a member of the
/// initial thread parameters struct (see `VSpace::prepare_thread`).
pub struct Consumer2<Role: CNodeRole, E, ESize: Unsigned, F, FSize: Unsigned, IRQ: Unsigned = U0>
where
    IRQ: IsLess<MaxIRQCount, Output = True>,
    ESize: IsGreater<U0, Output = True>,
    ESize: ArrayLength<Slot<E>>,
    FSize: IsGreater<U0, Output = True>,
    FSize: ArrayLength<Slot<F>>,
{
    irq_handler: Option<Cap<IRQHandler<IRQ, irq_state::Set>, Role>>,
    interrupt_badge: Badge,
    notification: Cap<Notification, Role>,
    queues: (
        (Badge, QueueHandle<E, Role, ESize>),
        (Badge, QueueHandle<F, Role, FSize>),
    ),
}

/// A multi-consumer that consumes interrupt-style notifications and from 3 queues
///
/// Designed to be handed to a new process as a member of the
/// initial thread parameters struct (see `VSpace::prepare_thread`).
pub struct Consumer3<
    Role: CNodeRole,
    E,
    ESize: Unsigned,
    F,
    FSize: Unsigned,
    G,
    GSize: Unsigned,
    IRQ: Unsigned = U0,
> where
    IRQ: IsLess<MaxIRQCount, Output = True>,
    ESize: IsGreater<U0, Output = True>,
    ESize: ArrayLength<Slot<E>>,
    FSize: IsGreater<U0, Output = True>,
    FSize: ArrayLength<Slot<F>>,
    GSize: IsGreater<U0, Output = True>,
    GSize: ArrayLength<Slot<G>>,
{
    irq_handler: Option<Cap<IRQHandler<IRQ, irq_state::Set>, Role>>,
    interrupt_badge: Badge,
    notification: Cap<Notification, Role>,
    queues: (
        (Badge, QueueHandle<E, Role, ESize>),
        (Badge, QueueHandle<F, Role, FSize>),
        (Badge, QueueHandle<G, Role, GSize>),
    ),
}

/// Wrapper around the necessary support and capabilities for a given
/// thread to push elements to an ingest queue for a multi-consumer
/// (e.g. `Consumer1`, `Consumer2`,etc).
///
/// Designed to be handed to a new process as a member of the
/// initial thread parameters struct (see `VSpace::prepare_thread`).
pub struct Producer<Role: CNodeRole, T: Sized + Sync + Send, QLen: Unsigned>
where
    QLen: IsGreater<U0, Output = True>,
    QLen: ArrayLength<Slot<T>>,
{
    notification: Cap<Notification, Role>,
    queue: QueueHandle<T, Role, QLen>,
}

struct QueueHandle<T: Sized, Role: CNodeRole, QLen: Unsigned>
where
    QLen: IsGreater<U0, Output = True>,
    QLen: ArrayLength<Slot<T>>,
{
    // Only valid in the VSpace context of a particular process
    shared_queue: usize,
    _role: PhantomData<Role>,
    _t: PhantomData<T>,
    _queue_len: PhantomData<QLen>,
}

/// Error relating to the creation of a multi-consumer or
/// its related ingest pathways.
#[derive(Debug)]
pub enum MultiConsumerError {
    QueueTooBig,
    ConsumerIdentityMismatch,
    ProduceToOwnQueueForbidden,
    SeL4Error(SeL4Error),
    VSpaceError(VSpaceError),
    RetypeError(RetypeError),
}

impl From<SeL4Error> for MultiConsumerError {
    fn from(s: SeL4Error) -> Self {
        MultiConsumerError::SeL4Error(s)
    }
}

impl From<VSpaceError> for MultiConsumerError {
    fn from(e: VSpaceError) -> Self {
        MultiConsumerError::VSpaceError(e)
    }
}

impl From<RetypeError> for MultiConsumerError {
    fn from(e: RetypeError) -> Self {
        MultiConsumerError::RetypeError(e)
    }
}

/// Wrapper around the necessary resources
/// to add a new producer to a given queue
/// ingested by a multi-consumer (e.g. `Consumer1`)
pub struct ProducerSetup<T, QLen: Unsigned> {
    shared_region: UnmappedMemoryRegion<PageBits, shared_status::Shared>,
    queue_badge: Badge,
    // User-concealed alias'ing happening here.
    // Don't mutate this Cap. Copying/minting is okay.
    notification: LocalCap<Notification>,
    consumer_vspace_asid: InternalASID,
    _queue_element_type: PhantomData<T>,
    _queue_lenth: PhantomData<QLen>,
}

/// Wrapper around the necessary resources
/// to trigger a multi-consumer's non-queue-reading
/// interrupt-like wakeup path.
pub struct WakerSetup {
    interrupt_badge: Badge,

    // User-concealed alias'ing happening here.
    // Don't mutate/delete this Cap. Copying/minting is okay.
    notification: LocalCap<Notification>,
}

/// Wrapper around the locally-accessible resources
/// needed to add more features to a `Consumer` instance,
/// such as adding an additional ingest queue.
pub struct ConsumerToken {
    // User-concealed alias'ing happening here.
    // Don't mutate/delete this Cap. Copying/minting is okay.
    notification: Cap<Notification, role::Local>,
    consumer_vspace_asid: Option<InternalASID>,
}

impl<IRQ: Unsigned> InterruptConsumer<IRQ, role::Child>
where
    IRQ: IsLess<MaxIRQCount, Output = True>,
{
    pub fn new(
        notification_ut: LocalCap<Untyped<<Notification as DirectRetype>::SizeBits>>,
        irq_control: &mut LocalCap<IRQControl>,
        local_cnode: &LocalCap<LocalCNode>,
        local_slots: LocalCNodeSlots<U3>,
        consumer_slots: ChildCNodeSlots<U2>,
    ) -> Result<(InterruptConsumer<IRQ, role::Child>, ConsumerToken), IRQError> {
        // Make a notification, mint-copy it to establish a badge
        let (local_slot, local_slots) = local_slots.alloc();
        let unbadged_notification: LocalCap<Notification> = notification_ut.retype(local_slot)?;

        let interrupt_badge = Badge::from(1);

        let (local_slot, local_slots) = local_slots.alloc();
        let notification =
            unbadged_notification.mint_inside_cnode(local_slot, CapRights::RWG, interrupt_badge)?;

        // Make a new IRQHandler, link it to the notification and move both to the child CNode
        let (local_slot, _local_slots) = local_slots.alloc();
        let irq_handler = irq_control.create_handler(local_slot)?;
        let irq_handler = irq_handler.set_notification(&notification)?;

        let (consumer_slot, consumer_slots) = consumer_slots.alloc();
        let irq_handler_in_child = irq_handler.move_to_slot(&local_cnode, consumer_slot)?;

        let (consumer_slot, _consumer_slots) = consumer_slots.alloc();
        let notification_in_child =
            notification.copy(&local_cnode, consumer_slot, CapRights::RW)?;
        Ok((
            InterruptConsumer {
                irq_handler: irq_handler_in_child,
                interrupt_badge,
                notification: notification_in_child,
            },
            ConsumerToken {
                notification,
                consumer_vspace_asid: None,
            },
        ))
    }

    pub fn add_queue<'a, 'b, ScratchPages: Unsigned, E: Sized + Send + Sync, ELen: Unsigned>(
        self,
        consumer_token: &mut ConsumerToken,
        shared_region_ut: LocalCap<Untyped<PageBits>>,
        local_vspace_scratch: &mut ScratchRegion<'a, 'b, ScratchPages>,
        consumer_vspace: &mut VSpace,
        local_cnode: &LocalCap<LocalCNode>,
        dest_slots: LocalCNodeSlots<U2>,
    ) -> Result<(Consumer1<role::Child, E, ELen, IRQ>, ProducerSetup<E, ELen>), MultiConsumerError>
    where
        ELen: ArrayLength<Slot<E>>,
        ELen: IsGreater<U0, Output = True>,
        ScratchPages: IsGreaterOrEqual<U1, Output = True>,
    {
        // The consumer token should not have a vspace associated with it at all yet, since
        // we have yet to require mapping any memory to it.
        if let Some(_) = consumer_token.consumer_vspace_asid {
            return Err(MultiConsumerError::ConsumerIdentityMismatch);
        }
        let (shared_region, consumer_shared_region) =
            create_region_filled_with_array_queue::<ScratchPages, E, ELen>(
                shared_region_ut,
                local_vspace_scratch,
                consumer_vspace,
                &local_cnode,
                dest_slots,
            )?;
        consumer_token.consumer_vspace_asid = Some(consumer_vspace.asid());

        // Assumes we are using the one-hot style for identifying the interrupt badge index
        let fresh_queue_badge = Badge::from(self.interrupt_badge.inner << 1);
        let producer_setup: ProducerSetup<E, ELen> = ProducerSetup {
            consumer_vspace_asid: consumer_vspace.asid(),
            shared_region,
            queue_badge: fresh_queue_badge,
            // Construct a user-inaccessible copy of the local notification
            // purely for use in producing child-cnode-residing copies.
            notification: Cap {
                cptr: consumer_token.notification.cptr,
                cap_data: PhantomCap::phantom_instance(),
                _role: PhantomData,
            },
            _queue_element_type: PhantomData,
            _queue_lenth: PhantomData,
        };

        Ok((
            Consumer1 {
                irq_handler: Some(self.irq_handler),
                interrupt_badge: self.interrupt_badge,
                notification: self.notification,
                queue_badge: fresh_queue_badge,
                queue: QueueHandle {
                    shared_queue: consumer_shared_region.vaddr(),
                    _role: PhantomData,
                    _t: PhantomData,
                    _queue_len: PhantomData,
                },
            },
            producer_setup,
        ))
    }
}

impl<E: Sized + Sync + Send, ELen: Unsigned, IRQ: Unsigned> Consumer1<role::Child, E, ELen, IRQ>
where
    IRQ: IsLess<MaxIRQCount, Output = True>,
    ELen: IsGreater<U0, Output = True>,
    ELen: ArrayLength<Slot<E>>,
{
    pub fn new<'a, 'b, ScratchPages: Unsigned>(
        notification_ut: LocalCap<Untyped<<Notification as DirectRetype>::SizeBits>>,
        shared_region_ut: LocalCap<Untyped<PageBits>>,
        local_vspace_scratch: &mut ScratchRegion<'a, 'b, ScratchPages>,
        consumer_vspace: &mut VSpace,
        local_cnode: &LocalCap<LocalCNode>,
        local_slots: LocalCNodeSlots<U3>,
        consumer_slot: ChildCNodeSlots<U1>,
    ) -> Result<
        (
            Consumer1<role::Child, E, ELen, IRQ>,
            ConsumerToken,
            ProducerSetup<E, ELen>,
            WakerSetup,
        ),
        MultiConsumerError,
    >
    where
        ELen: ArrayLength<Slot<E>>,
        ELen: IsGreater<U0, Output = True>,
        ScratchPages: IsGreaterOrEqual<U1, Output = True>,
    {
        let queue_size = core::mem::size_of::<ArrayQueue<E, ELen>>();
        if queue_size > PageBytes::USIZE {
            return Err(MultiConsumerError::QueueTooBig);
        }
        let (slots, local_slots) = local_slots.alloc();
        let (shared_region, consumer_shared_region) =
            create_region_filled_with_array_queue::<ScratchPages, E, ELen>(
                shared_region_ut,
                local_vspace_scratch,
                consumer_vspace,
                &local_cnode,
                slots,
            )?;

        let (slot, _local_slots) = local_slots.alloc();
        let local_notification: LocalCap<Notification> = notification_ut.retype(slot)?;

        let consumer_notification = local_notification.mint(
            &local_cnode,
            consumer_slot,
            CapRights::RWG,
            Badge::from(0x00), // Only for Wait'ing, no need to set badge bits
        )?;
        let interrupt_badge = Badge::from(1 << 0);
        let queue_badge = Badge::from(1 << 1);

        let producer_setup: ProducerSetup<E, ELen> = ProducerSetup {
            consumer_vspace_asid: consumer_vspace.asid(),
            shared_region,
            queue_badge: queue_badge,
            // Construct a user-inaccessible copy of the local notification
            // purely for use in producing child-cnode-residing copies.
            notification: Cap {
                cptr: local_notification.cptr,
                cap_data: PhantomCap::phantom_instance(),
                _role: PhantomData,
            },
            _queue_element_type: PhantomData,
            _queue_lenth: PhantomData,
        };
        let consumer_token = ConsumerToken {
            // Construct a user-inaccessible copy of the local notification
            // purely for use in producing child-cnode-residing copies.
            notification: Cap {
                cptr: local_notification.cptr,
                cap_data: PhantomCap::phantom_instance(),
                _role: PhantomData,
            },
            consumer_vspace_asid: Some(consumer_vspace.asid()),
        };
        let waker_setup = WakerSetup {
            interrupt_badge,
            notification: local_notification,
        };
        Ok((
            Consumer1 {
                irq_handler: None,
                interrupt_badge,
                queue_badge,
                notification: consumer_notification,
                queue: QueueHandle {
                    shared_queue: consumer_shared_region.vaddr(),
                    _role: PhantomData,
                    _t: PhantomData,
                    _queue_len: PhantomData,
                },
            },
            consumer_token,
            producer_setup,
            waker_setup,
        ))
    }

    pub fn add_queue<'a, 'b, ScratchPages: Unsigned, F: Sized + Send + Sync, FLen: Unsigned>(
        self,
        consumer_token: &ConsumerToken,
        shared_region_ut: LocalCap<Untyped<PageBits>>,
        local_vspace_scratch: &mut ScratchRegion<'a, 'b, ScratchPages>,
        consumer_vspace: &mut VSpace,
        local_cnode: &LocalCap<LocalCNode>,
        dest_slots: LocalCNodeSlots<U2>,
    ) -> Result<
        (
            Consumer2<role::Child, E, ELen, F, FLen, IRQ>,
            ProducerSetup<F, FLen>,
        ),
        MultiConsumerError,
    >
    where
        FLen: ArrayLength<Slot<F>>,
        FLen: IsGreater<U0, Output = True>,
        ScratchPages: IsGreaterOrEqual<U1, Output = True>,
    {
        // Ensure that the consumer process that the `waker_setup` is wrapping
        // a notification to is the same process as the one referred to by
        // the `consumer_vspace` parameter.
        if let Some(ref consumer_token_vspace_asid) = consumer_token.consumer_vspace_asid {
            if consumer_token_vspace_asid != &consumer_vspace.asid() {
                return Err(MultiConsumerError::ConsumerIdentityMismatch);
            }
        } else {
            return Err(MultiConsumerError::ConsumerIdentityMismatch);
        }
        let (shared_region, consumer_shared_region) =
            create_region_filled_with_array_queue::<ScratchPages, F, FLen>(
                shared_region_ut,
                local_vspace_scratch,
                consumer_vspace,
                &local_cnode,
                dest_slots,
            )?;

        let fresh_queue_badge = Badge::from(self.queue_badge.inner << 1);
        let producer_setup: ProducerSetup<F, FLen> = ProducerSetup {
            consumer_vspace_asid: consumer_vspace.asid(),
            shared_region,
            queue_badge: fresh_queue_badge,
            // Construct a user-inaccessible copy of the local notification
            // purely for use in producing child-cnode-residing copies.
            notification: Cap {
                cptr: consumer_token.notification.cptr,
                cap_data: PhantomCap::phantom_instance(),
                _role: PhantomData,
            },
            _queue_element_type: PhantomData,
            _queue_lenth: PhantomData,
        };
        Ok((
            Consumer2 {
                irq_handler: None,
                interrupt_badge: self.interrupt_badge,
                notification: self.notification,
                queues: (
                    (self.queue_badge, self.queue),
                    (
                        fresh_queue_badge,
                        QueueHandle {
                            shared_queue: consumer_shared_region.vaddr(),
                            _role: PhantomData,
                            _t: PhantomData,
                            _queue_len: PhantomData,
                        },
                    ),
                ),
            },
            producer_setup,
        ))
    }
}

impl<
        E: Sized + Sync + Send,
        ELen: Unsigned,
        F: Sized + Sync + Send,
        FLen: Unsigned,
        IRQ: Unsigned,
    > Consumer2<role::Child, E, ELen, F, FLen, IRQ>
where
    IRQ: IsLess<MaxIRQCount, Output = True>,
    ELen: IsGreater<U0, Output = True>,
    ELen: ArrayLength<Slot<E>>,
    FLen: IsGreater<U0, Output = True>,
    FLen: ArrayLength<Slot<F>>,
{
    pub fn add_queue<
        'a,
        'b,
        ScratchPages: Unsigned,
        G: Sized + Send + Sync,
        GLen: Unsigned,
        LocalCNodeFreeSlots: Unsigned,
    >(
        self,
        consumer_token: &ConsumerToken,
        shared_region_ut: LocalCap<Untyped<PageBits>>,
        local_vspace_scratch: &mut ScratchRegion<'a, 'b, ScratchPages>,
        consumer_vspace: &mut VSpace,
        local_cnode: &LocalCap<LocalCNode>,
        dest_slots: LocalCNodeSlots<U2>,
    ) -> Result<
        (
            Consumer3<role::Child, E, ELen, F, FLen, G, GLen, IRQ>,
            ProducerSetup<F, FLen>,
        ),
        MultiConsumerError,
    >
    where
        FLen: ArrayLength<Slot<F>>,
        FLen: IsGreater<U0, Output = True>,
        GLen: ArrayLength<Slot<G>>,
        GLen: IsGreater<U0, Output = True>,
        ScratchPages: IsGreaterOrEqual<U1, Output = True>,
    {
        // Ensure that the consumer process that the `waker_setup` is wrapping
        // a notification to is the same process as the one referred to by
        // the `consumer_vspace` parameter.
        if let Some(ref consumer_token_vspace_asid) = consumer_token.consumer_vspace_asid {
            if consumer_token_vspace_asid != &consumer_vspace.asid() {
                return Err(MultiConsumerError::ConsumerIdentityMismatch);
            }
        } else {
            return Err(MultiConsumerError::ConsumerIdentityMismatch);
        }
        let (shared_region, consumer_shared_region) =
            create_region_filled_with_array_queue::<ScratchPages, F, FLen>(
                shared_region_ut,
                local_vspace_scratch,
                consumer_vspace,
                &local_cnode,
                dest_slots,
            )?;

        let fresh_queue_badge = Badge::from((self.queues.1).0.inner << 1);
        let producer_setup: ProducerSetup<F, FLen> = ProducerSetup {
            consumer_vspace_asid: consumer_vspace.asid(),
            shared_region,
            queue_badge: fresh_queue_badge,
            // Construct a user-inaccessible copy of the local notification
            // purely for use in producing child-cnode-residing copies.
            notification: Cap {
                cptr: consumer_token.notification.cptr,
                cap_data: PhantomCap::phantom_instance(),
                _role: PhantomData,
            },
            _queue_element_type: PhantomData,
            _queue_lenth: PhantomData,
        };
        Ok((
            Consumer3 {
                irq_handler: None,
                interrupt_badge: self.interrupt_badge,
                notification: self.notification,
                queues: (
                    self.queues.0,
                    self.queues.1,
                    (
                        fresh_queue_badge,
                        QueueHandle {
                            shared_queue: consumer_shared_region.vaddr(),
                            _role: PhantomData,
                            _t: PhantomData,
                            _queue_len: PhantomData,
                        },
                    ),
                ),
            },
            producer_setup,
        ))
    }
}

fn create_region_filled_with_array_queue<
    'a,
    'b,
    ScratchPages: Unsigned,
    T: Sized + Send + Sync,
    QLen: Unsigned,
>(
    shared_region_ut: LocalCap<Untyped<PageBits>>,
    local_vspace_scratch: &mut ScratchRegion<'a, 'b, ScratchPages>,
    consumer_vspace: &mut VSpace,
    local_cnode: &LocalCap<LocalCNode>,
    dest_slots: LocalCNodeSlots<U2>,
) -> Result<
    (
        UnmappedMemoryRegion<PageBits, shared_status::Shared>,
        MappedMemoryRegion<PageBits, shared_status::Shared>,
    ),
    MultiConsumerError,
>
where
    QLen: ArrayLength<Slot<T>>,
    QLen: IsGreater<U0, Output = True>,
    ScratchPages: IsGreaterOrEqual<U1, Output = True>,
{
    let queue_size = core::mem::size_of::<ArrayQueue<T, QLen>>();
    if queue_size > PageBytes::USIZE {
        return Err(MultiConsumerError::QueueTooBig);
    }

    let (slot, dest_slots) = dest_slots.alloc();
    let mut region = UnmappedMemoryRegion::new(shared_region_ut, slot)?;
    // Put some data in there. Specifically, an `ArrayQueue`.
    local_vspace_scratch.temporarily_map_region(&mut region, |mapped_region| unsafe {
        let aq_ptr = core::mem::transmute::<usize, *mut ArrayQueue<T, QLen>>(mapped_region.vaddr());
        // Operate directly on a pointer to an uninitialized/zeroed pointer
        // in order to reduces odds of the full ArrayQueue instance
        // materializing all at once on the local stack (potentially blowing it)
        ArrayQueue::<T, QLen>::new_at_ptr(aq_ptr);
        core::mem::forget(aq_ptr);
    })?;

    let (shared_slot, _) = dest_slots.alloc();
    let shared_region = region.to_shared();
    let consumer_shared_region = consumer_vspace.map_shared_region(
        &shared_region,
        CapRights::RW,
        arch::vm_attributes::DEFAULT,
        shared_slot,
        local_cnode,
    )?;
    Ok((shared_region, consumer_shared_region))
}

/// Wrapper around the necessary capabilities for a given
/// thread to awaken a multi-consumer to run the "non-queue-reading wakeup" path.
///
/// Designed to be handed to a new process as a member of the
/// initial thread parameters struct (see `VSpace::prepare_thread`).
pub struct Waker<Role: CNodeRole> {
    notification: Cap<Notification, Role>,
}

impl Waker<role::Child> {
    pub fn new(
        setup: &WakerSetup,
        dest_slot: ChildCNodeSlot,
        local_cnode: &LocalCap<LocalCNode>,
    ) -> Result<Self, SeL4Error> {
        let notification = setup.notification.mint(
            local_cnode,
            dest_slot,
            CapRights::RWG,
            setup.interrupt_badge,
        )?;
        Ok(Waker { notification })
    }
}

impl Waker<role::Local> {
    /// Let the multi-consumer know that it ought to run its "interrupt" path.
    pub fn send_wakeup_signal(&self) {
        unsafe {
            seL4_Signal(self.notification.cptr);
        }
    }
}

impl<IRQ: Unsigned> InterruptConsumer<IRQ, role::Local>
where
    IRQ: IsLess<MaxIRQCount, Output = True>,
{
    pub fn consume<State, WFn>(self, initial_state: State, mut waker_fn: WFn) -> !
    where
        WFn: FnMut(State) -> State,
    {
        let mut sender_badge: usize = 0;
        let mut state = initial_state;
        // Run an initial ack to clear out interrupt state ahead of waiting
        match self.irq_handler.ack() {
            Ok(_) => (),
            Err(e) => {
                debug_println!("Ack error in InterruptConsumer::consume setup. {:?}", e);
                panic!()
            }
        };
        loop {
            unsafe {
                seL4_Wait(self.notification.cptr, &mut sender_badge as *mut usize);
                let current_badge = Badge::from(sender_badge);
                if self
                    .interrupt_badge
                    .are_all_overlapping_bits_set(current_badge)
                {
                    state = waker_fn(state);
                    match self.irq_handler.ack() {
                        Ok(_) => (),
                        Err(e) => {
                            debug_println!("Ack error in InterruptConsumer::consume loop. {:?}", e);
                            panic!()
                        }
                    };
                } else {
                    debug_println!(
                        "Unexpected badge in InterruptConsumer::consume loop. {:?}",
                        current_badge
                    );
                    panic!()
                }
            }
        }
    }
}
impl<E: Sized + Sync + Send, QLen: Unsigned> Consumer1<role::Local, E, QLen>
where
    QLen: IsGreater<U0, Output = True>,
    QLen: ArrayLength<Slot<E>>,
{
    pub fn consume<State, WFn, EFn>(self, initial_state: State, waker_fn: WFn, queue_fn: EFn) -> !
    where
        WFn: Fn(State) -> State,
        EFn: Fn(E, State) -> State,
    {
        let mut sender_badge: usize = 0;
        let mut state = initial_state;
        let queue: &mut ArrayQueue<E, QLen> =
            unsafe { core::mem::transmute(self.queue.shared_queue as *mut ArrayQueue<E, QLen>) };
        if let Some(ref irq_handler) = self.irq_handler {
            // Run an initial ack to clear out interrupt state ahead of waiting
            match irq_handler.ack() {
                Ok(_) => (),
                Err(e) => {
                    debug_println!("Ack error in InterruptConsumer::consume setup. {:?}", e);
                    panic!()
                }
            };
        }
        loop {
            unsafe {
                seL4_Wait(self.notification.cptr, &mut sender_badge as *mut usize);
                let current_badge = Badge::from(sender_badge);
                if self
                    .interrupt_badge
                    .are_all_overlapping_bits_set(current_badge)
                {
                    state = waker_fn(state);
                    if let Some(ref irq_handler) = self.irq_handler {
                        match irq_handler.ack() {
                            Ok(_) => (),
                            Err(e) => {
                                debug_println!(
                                    "Ack error in InterruptConsumer::consume loop. {:?}",
                                    e
                                );
                                panic!()
                            }
                        };
                    }
                }
                if self.queue_badge.are_all_overlapping_bits_set(current_badge) {
                    for _ in 0..QLen::USIZE.saturating_add(1) {
                        if let Ok(e) = queue.pop() {
                            state = queue_fn(e, state);
                        } else {
                            break;
                        }
                    }
                }
            }
        }
    }
}

impl<E: Sized + Sync + Send, ELen: Unsigned, F: Sized + Sync + Send, FLen: Unsigned>
    Consumer2<role::Local, E, ELen, F, FLen>
where
    ELen: IsGreater<U0, Output = True>,
    ELen: ArrayLength<Slot<E>>,
    FLen: IsGreater<U0, Output = True>,
    FLen: ArrayLength<Slot<F>>,
{
    pub fn consume<State, WFn, EFn, FFn>(
        self,
        initial_state: State,
        waker_fn: WFn,
        queue_e_fn: EFn,
        queue_f_fn: FFn,
    ) -> !
    where
        WFn: Fn(State) -> State,
        EFn: Fn(E, State) -> State,
        FFn: Fn(F, State) -> State,
    {
        let mut sender_badge: usize = 0;
        let mut state = initial_state;
        let (badge_e, handle_e) = self.queues.0;
        let queue_e: &mut ArrayQueue<E, ELen> =
            unsafe { core::mem::transmute(handle_e.shared_queue as *mut ArrayQueue<E, ELen>) };
        let (badge_f, handle_f) = self.queues.1;
        let queue_f: &mut ArrayQueue<F, FLen> =
            unsafe { core::mem::transmute(handle_f.shared_queue as *mut ArrayQueue<F, FLen>) };
        if let Some(ref irq_handler) = self.irq_handler {
            match irq_handler.ack() {
                Ok(_) => (),
                Err(e) => {
                    debug_println!("Ack error in InterruptConsumer::consume setup. {:?}", e);
                    panic!()
                }
            };
        }
        loop {
            unsafe {
                seL4_Wait(self.notification.cptr, &mut sender_badge as *mut usize);
                let current_badge = Badge::from(sender_badge);
                if self
                    .interrupt_badge
                    .are_all_overlapping_bits_set(current_badge)
                {
                    state = waker_fn(state);
                    if let Some(ref irq_handler) = self.irq_handler {
                        match irq_handler.ack() {
                            Ok(_) => (),
                            Err(e) => {
                                debug_println!(
                                    "Ack error in InterruptConsumer::consume loop. {:?}",
                                    e
                                );
                                panic!()
                            }
                        };
                    }
                }
                if badge_e.are_all_overlapping_bits_set(current_badge) {
                    for _ in 0..ELen::USIZE.saturating_add(1) {
                        if let Ok(e) = queue_e.pop() {
                            state = queue_e_fn(e, state);
                        } else {
                            break;
                        }
                    }
                }
                if badge_f.are_all_overlapping_bits_set(current_badge) {
                    for _ in 0..FLen::USIZE.saturating_add(1) {
                        if let Ok(e) = queue_f.pop() {
                            state = queue_f_fn(e, state);
                        } else {
                            break;
                        }
                    }
                }
            }
        }
    }
}

impl<
        E: Sized + Sync + Send,
        ELen: Unsigned,
        F: Sized + Sync + Send,
        FLen: Unsigned,
        G: Sized + Sync + Send,
        GLen: Unsigned,
    > Consumer3<role::Local, E, ELen, F, FLen, G, GLen>
where
    ELen: IsGreater<U0, Output = True>,
    ELen: ArrayLength<Slot<E>>,
    FLen: IsGreater<U0, Output = True>,
    FLen: ArrayLength<Slot<F>>,
    GLen: IsGreater<U0, Output = True>,
    GLen: ArrayLength<Slot<G>>,
{
    pub fn consume<State, WFn, EFn, FFn, GFn>(
        self,
        initial_state: State,
        waker_fn: WFn,
        queue_e_fn: EFn,
        queue_f_fn: FFn,
        queue_g_fn: GFn,
    ) -> !
    where
        WFn: Fn(State) -> State,
        EFn: Fn(E, State) -> State,
        FFn: Fn(F, State) -> State,
        GFn: Fn(G, State) -> State,
    {
        let mut sender_badge: usize = 0;
        let mut state = initial_state;
        let (badge_e, handle_e) = self.queues.0;
        let queue_e: &mut ArrayQueue<E, ELen> =
            unsafe { core::mem::transmute(handle_e.shared_queue as *mut ArrayQueue<E, ELen>) };
        let (badge_f, handle_f) = self.queues.1;
        let queue_f: &mut ArrayQueue<F, FLen> =
            unsafe { core::mem::transmute(handle_f.shared_queue as *mut ArrayQueue<F, FLen>) };
        let (badge_g, handle_g) = self.queues.2;
        let queue_g: &mut ArrayQueue<G, GLen> =
            unsafe { core::mem::transmute(handle_g.shared_queue as *mut ArrayQueue<G, GLen>) };
        if let Some(ref irq_handler) = self.irq_handler {
            match irq_handler.ack() {
                Ok(_) => (),
                Err(e) => {
                    debug_println!("Ack error in InterruptConsumer::consume setup. {:?}", e);
                    panic!()
                }
            };
        }
        loop {
            unsafe {
                seL4_Wait(self.notification.cptr, &mut sender_badge as *mut usize);
                let current_badge = Badge::from(sender_badge);
                if self
                    .interrupt_badge
                    .are_all_overlapping_bits_set(current_badge)
                {
                    state = waker_fn(state);
                    if let Some(ref irq_handler) = self.irq_handler {
                        match irq_handler.ack() {
                            Ok(_) => (),
                            Err(e) => {
                                debug_println!(
                                    "Ack error in InterruptConsumer::consume loop. {:?}",
                                    e
                                );
                                panic!()
                            }
                        };
                    }
                }
                if badge_e.are_all_overlapping_bits_set(current_badge) {
                    for _ in 0..ELen::USIZE.saturating_add(1) {
                        if let Ok(e) = queue_e.pop() {
                            state = queue_e_fn(e, state);
                        } else {
                            break;
                        }
                    }
                }
                if badge_f.are_all_overlapping_bits_set(current_badge) {
                    for _ in 0..FLen::USIZE.saturating_add(1) {
                        if let Ok(e) = queue_f.pop() {
                            state = queue_f_fn(e, state);
                        } else {
                            break;
                        }
                    }
                }
                if badge_g.are_all_overlapping_bits_set(current_badge) {
                    for _ in 0..FLen::USIZE.saturating_add(1) {
                        if let Ok(e) = queue_g.pop() {
                            state = queue_g_fn(e, state);
                        } else {
                            break;
                        }
                    }
                }
            }
        }
    }
}

impl<T: Sized + Sync + Send, QLen: Unsigned> Producer<role::Child, T, QLen>
where
    QLen: IsGreater<U0, Output = True>,
    QLen: ArrayLength<Slot<T>>,
{
    pub fn new(
        setup: &ProducerSetup<T, QLen>,
        dest_slot: ChildCNodeSlot,
        child_vspace: &mut VSpace,
        local_cnode: &LocalCap<LocalCNode>,
        local_slot: LocalCNodeSlot,
    ) -> Result<Self, MultiConsumerError> {
        if setup.consumer_vspace_asid == child_vspace.asid() {
            // To simplify reasoning about likely control flow patterns,
            // we presently disallow a consumer thread from producing to one
            // of its own ingest queues.
            return Err(MultiConsumerError::ProduceToOwnQueueForbidden);
        }
        let producer_shared_region = child_vspace.map_shared_region(
            &setup.shared_region,
            CapRights::RW,
            arch::vm_attributes::DEFAULT,
            local_slot,
            &local_cnode,
        )?;
        let notification =
            setup
                .notification
                .mint(&local_cnode, dest_slot, CapRights::RWG, setup.queue_badge)?;
        Ok(Producer {
            notification,
            queue: QueueHandle {
                shared_queue: producer_shared_region.vaddr(),
                _role: PhantomData,
                _t: PhantomData,
                _queue_len: PhantomData,
            },
        })
    }
}

/// Error which occurs when pushing into a full queue.
#[derive(Clone, Copy, Eq, PartialEq)]
pub struct QueueFullError<T>(pub T);

impl<T> From<PushError<T>> for QueueFullError<T> {
    fn from(p: PushError<T>) -> Self {
        QueueFullError(p.0)
    }
}

impl<T: Sized + Sync + Send, QLen: Unsigned> Producer<role::Local, T, QLen>
where
    QLen: IsGreater<U0, Output = True>,
    QLen: ArrayLength<Slot<T>>,
{
    pub fn send(&self, t: T) -> Result<(), QueueFullError<T>> {
        let queue: &mut ArrayQueue<T, QLen> =
            unsafe { core::mem::transmute(self.queue.shared_queue as *mut ArrayQueue<T, QLen>) };
        queue.push(t)?;
        unsafe { seL4_Signal(self.notification.cptr) }
        Ok(())
    }
}
