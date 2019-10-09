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
use core::mem::size_of;
use core::ops::{Sub};

use cross_queue::{ArrayQueue, PushError, Slot};
use generic_array::ArrayLength;
use selfe_sys::{seL4_Signal, seL4_Wait};
use typenum::*;

use crate::arch::{self, PageBits};
use crate::cap::{
    irq_state, role, Badge, CNodeRole, CNodeSlot, Cap, ChildCNodeSlot, ChildCNodeSlots,
    DirectRetype, IRQControl, IRQError, IRQHandler, InternalASID, LocalCNode, LocalCNodeSlot,
    LocalCNodeSlots, LocalCap, MaxIRQCount, Notification, PhantomCap, Untyped,
};
use crate::error::SeL4Error;
use crate::pow::{Pow, _Pow};
use crate::userland::CapRights;
use crate::vspace::{
    shared_status, KernelRetypeFanOutLimit, MappedMemoryRegion, NumPages, ScratchRegion,
    UnmappedMemoryRegion, VSpace, VSpaceError,
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
pub struct Consumer1<Role: CNodeRole, T: Sized + Sync + Send, IRQ: Unsigned = U0>
where
    IRQ: IsLess<MaxIRQCount, Output = True>,
{
    irq_handler: Option<Cap<IRQHandler<IRQ, irq_state::Set>, Role>>,
    interrupt_badge: Badge,
    notification: Cap<Notification, Role>,
    queue_badge: Badge,
    queue: QueueHandle<T, Role>,
}

/// A multi-consumer that consumes interrupt-style notifications and from 2 queues
///
/// Designed to be handed to a new process as a member of the
/// initial thread parameters struct (see `VSpace::prepare_thread`).
pub struct Consumer2<Role: CNodeRole, E, F, IRQ: Unsigned = U0>
where
    IRQ: IsLess<MaxIRQCount, Output = True>,
{
    irq_handler: Option<Cap<IRQHandler<IRQ, irq_state::Set>, Role>>,
    interrupt_badge: Badge,
    notification: Cap<Notification, Role>,
    queues: ((Badge, QueueHandle<E, Role>), (Badge, QueueHandle<F, Role>)),
}

/// A multi-consumer that consumes interrupt-style notifications and from 3 queues
///
/// Designed to be handed to a new process as a member of the
/// initial thread parameters struct (see `VSpace::prepare_thread`).
pub struct Consumer3<Role: CNodeRole, E, F, G, IRQ: Unsigned = U0>
where
    IRQ: IsLess<MaxIRQCount, Output = True>,
{
    irq_handler: Option<Cap<IRQHandler<IRQ, irq_state::Set>, Role>>,
    interrupt_badge: Badge,
    notification: Cap<Notification, Role>,
    queues: (
        (Badge, QueueHandle<E, Role>),
        (Badge, QueueHandle<F, Role>),
        (Badge, QueueHandle<G, Role>),
    ),
}

/// Wrapper around the necessary support and capabilities for a given
/// thread to push elements to an ingest queue for a multi-consumer
/// (e.g. `Consumer1`, `Consumer2`,etc).
///
/// Designed to be handed to a new process as a member of the
/// initial thread parameters struct (see `VSpace::prepare_thread`).
pub struct Producer<Role: CNodeRole, T: Sized + Sync + Send> {
    notification: Cap<Notification, Role>,
    queue: QueueHandle<T, Role>,
}

struct QueueHandle<T: Sized, Role: CNodeRole> {
    // Only valid in the VSpace context of a particular process
    shared_queue: usize,
    queue_len: usize,
    _role: PhantomData<Role>,
    _t: PhantomData<T>,
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

/// Wrapper around the necessary resources
/// to add a new producer to a given queue
/// ingested by a multi-consumer (e.g. `Consumer1`)
pub struct ProducerSetup<T, QLen: Unsigned, QSizeBits: Unsigned>
where
    // needed for memoryregion
    QSizeBits: IsGreaterOrEqual<PageBits>,
    QSizeBits: Sub<PageBits>,
    <QSizeBits as Sub<PageBits>>::Output: Unsigned,
    <QSizeBits as Sub<PageBits>>::Output: _Pow,
    Pow<<QSizeBits as Sub<PageBits>>::Output>: Unsigned,
{
    shared_region: UnmappedMemoryRegion<QSizeBits, shared_status::Shared>,
    queue_badge: Badge,
    // User-concealed alias'ing happening here.
    // Don't mutate this Cap. Copying/minting is okay.
    notification: LocalCap<Notification>,
    consumer_vspace_asid: InternalASID,
    _queue_element_type: PhantomData<T>,
    _queue_length: PhantomData<QLen>,
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
                notification: unbadged_notification,
                consumer_vspace_asid: None,
            },
        ))
    }

    pub fn add_queue<
        E: Sized + Send + Sync,
        ELen: Unsigned,
        EQueueSizeBits: Unsigned,
        ScratchPages: Unsigned,
    >(
        self,
        consumer_token: &mut ConsumerToken,
        shared_region_ut: LocalCap<Untyped<EQueueSizeBits>>,
        local_vspace_scratch: &mut ScratchRegion<ScratchPages>,
        consumer_vspace: &mut VSpace,
        local_cnode: &LocalCap<LocalCNode>,
        umr_slots: LocalCNodeSlots<NumPages<EQueueSizeBits>>,
        shared_slots: LocalCNodeSlots<NumPages<EQueueSizeBits>>,
    ) -> Result<
        (
            Consumer1<role::Child, E, IRQ>,
            ProducerSetup<E, ELen, EQueueSizeBits>,
        ),
        MultiConsumerError,
    >
    where
        ELen: ArrayLength<Slot<E>>,
        ELen: IsGreater<U0, Output = True>,
        ScratchPages: IsGreaterOrEqual<NumPages<EQueueSizeBits>, Output = True>,

        // needed for memoryregion
        EQueueSizeBits: IsGreaterOrEqual<PageBits>,
        EQueueSizeBits: Sub<PageBits>,
        <EQueueSizeBits as Sub<PageBits>>::Output: Unsigned,
        <EQueueSizeBits as Sub<PageBits>>::Output: _Pow,
        Pow<<EQueueSizeBits as Sub<PageBits>>::Output>:
            Unsigned + IsGreaterOrEqual<U1, Output = True>,

        // needed for unmappedMemoryRegion constructor
        Pow<<EQueueSizeBits as Sub<PageBits>>::Output>:
            IsLessOrEqual<KernelRetypeFanOutLimit, Output = True>,
    {
        // The consumer token should not have a vspace associated with it at all yet, since
        // we have yet to require mapping any memory to it.
        if let Some(_) = consumer_token.consumer_vspace_asid {
            return Err(MultiConsumerError::ConsumerIdentityMismatch);
        }
        let (shared_region, consumer_shared_region) =
            create_region_filled_with_array_queue::<ScratchPages, E, ELen, EQueueSizeBits>(
                shared_region_ut,
                local_vspace_scratch,
                consumer_vspace,
                &local_cnode,
                umr_slots,
                shared_slots,
            )?;
        consumer_token.consumer_vspace_asid = Some(consumer_vspace.asid());

        // Assumes we are using the one-hot style for identifying the interrupt badge index
        let fresh_queue_badge = Badge::from(self.interrupt_badge.inner << 1);
        let producer_setup: ProducerSetup<E, ELen, EQueueSizeBits> = ProducerSetup {
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
            _queue_length: PhantomData,
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
                    queue_len: ELen::USIZE,
                },
            },
            producer_setup,
        ))
    }
}

impl<E: Sized + Sync + Send, IRQ: Unsigned> Consumer1<role::Child, E, IRQ>
where
    IRQ: IsLess<MaxIRQCount, Output = True>,
{
    pub fn new<ELen: Unsigned, EQueueSizeBits: Unsigned, ScratchPages: Unsigned>(
        notification_ut: LocalCap<Untyped<<Notification as DirectRetype>::SizeBits>>,
        shared_region_ut: LocalCap<Untyped<EQueueSizeBits>>,
        local_vspace_scratch: &mut ScratchRegion<ScratchPages>,
        consumer_vspace: &mut VSpace,
        local_cnode: &LocalCap<LocalCNode>,
        umr_slots: LocalCNodeSlots<NumPages<EQueueSizeBits>>,
        shared_slots: LocalCNodeSlots<NumPages<EQueueSizeBits>>,
        notification_slot: LocalCNodeSlot,
        consumer_slot: ChildCNodeSlot,
    ) -> Result<
        (
            Consumer1<role::Child, E, IRQ>,
            ConsumerToken,
            ProducerSetup<E, ELen, EQueueSizeBits>,
            WakerSetup,
        ),
        MultiConsumerError,
    >
    where
        ELen: ArrayLength<Slot<E>>,
        ELen: IsGreater<U0, Output = True>,
        EQueueSizeBits: Unsigned,
        ScratchPages: IsGreaterOrEqual<NumPages<EQueueSizeBits>, Output = True>,

        // needed for memoryregion
        EQueueSizeBits: IsGreaterOrEqual<PageBits>,
        EQueueSizeBits: Sub<PageBits>,
        <EQueueSizeBits as Sub<PageBits>>::Output: Unsigned,
        <EQueueSizeBits as Sub<PageBits>>::Output: _Pow,
        Pow<<EQueueSizeBits as Sub<PageBits>>::Output>:
            Unsigned + IsGreaterOrEqual<U1, Output = True>,

        // needed for unmappedMemoryRegion constructor
        Pow<<EQueueSizeBits as Sub<PageBits>>::Output>:
            IsLessOrEqual<KernelRetypeFanOutLimit, Output = True>,
    {
        let (shared_region, consumer_shared_region) =
            create_region_filled_with_array_queue::<ScratchPages, E, ELen, EQueueSizeBits>(
                shared_region_ut,
                local_vspace_scratch,
                consumer_vspace,
                &local_cnode,
                umr_slots,
                shared_slots,
            )?;

        let local_notification: LocalCap<Notification> =
            notification_ut.retype(notification_slot)?;

        let consumer_notification = local_notification.mint(
            &local_cnode,
            consumer_slot,
            CapRights::RWG,
            Badge::from(0x00), // Only for Wait'ing, no need to set badge bits
        )?;
        let interrupt_badge = Badge::from(1 << 0);
        let queue_badge = Badge::from(1 << 1);

        let producer_setup: ProducerSetup<E, ELen, EQueueSizeBits> = ProducerSetup {
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
            _queue_length: PhantomData,
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
                    queue_len: ELen::USIZE,
                },
            },
            consumer_token,
            producer_setup,
            waker_setup,
        ))
    }

    pub fn add_queue<
        F: Sized + Send + Sync,
        FLen: Unsigned,
        FQueueSizeBits: Unsigned,
        ScratchPages: Unsigned,
    >(
        self,
        consumer_token: &ConsumerToken,
        shared_region_ut: LocalCap<Untyped<FQueueSizeBits>>,
        local_vspace_scratch: &mut ScratchRegion<ScratchPages>,
        consumer_vspace: &mut VSpace,
        local_cnode: &LocalCap<LocalCNode>,
        umr_slots: LocalCNodeSlots<NumPages<FQueueSizeBits>>,
        shared_slots: LocalCNodeSlots<NumPages<FQueueSizeBits>>,
    ) -> Result<
        (
            Consumer2<role::Child, E, F, IRQ>,
            ProducerSetup<F, FLen, FQueueSizeBits>,
        ),
        MultiConsumerError,
    >
    where
        FLen: ArrayLength<Slot<F>>,
        FLen: IsGreater<U0, Output = True>,
        ScratchPages: IsGreaterOrEqual<NumPages<FQueueSizeBits>, Output = True>,

        // needed for memoryregion
        FQueueSizeBits: IsGreaterOrEqual<PageBits>,
        FQueueSizeBits: Sub<PageBits>,
        <FQueueSizeBits as Sub<PageBits>>::Output: Unsigned,
        <FQueueSizeBits as Sub<PageBits>>::Output: _Pow,
        Pow<<FQueueSizeBits as Sub<PageBits>>::Output>:
            Unsigned + IsGreaterOrEqual<U1, Output = True>,

        // needed for unmappedMemoryRegion constructor
        Pow<<FQueueSizeBits as Sub<PageBits>>::Output>:
            IsLessOrEqual<KernelRetypeFanOutLimit, Output = True>,
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
            create_region_filled_with_array_queue::<ScratchPages, F, FLen, FQueueSizeBits>(
                shared_region_ut,
                local_vspace_scratch,
                consumer_vspace,
                &local_cnode,
                umr_slots,
                shared_slots,
            )?;

        let fresh_queue_badge = Badge::from(self.queue_badge.inner << 1);
        let producer_setup: ProducerSetup<F, FLen, FQueueSizeBits> = ProducerSetup {
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
            _queue_length: PhantomData,
        };
        Ok((
            Consumer2 {
                irq_handler: self.irq_handler,
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
                            queue_len: FLen::USIZE,
                        },
                    ),
                ),
            },
            producer_setup,
        ))
    }
}

impl<E: Sized + Sync + Send, F: Sized + Sync + Send, IRQ: Unsigned>
    Consumer2<role::Child, E, F, IRQ>
where
    IRQ: IsLess<MaxIRQCount, Output = True>,
{
    pub fn add_queue<
        ScratchPages: Unsigned,
        G: Sized + Send + Sync,
        GLen: Unsigned,
        GQueueSizeBits: Unsigned,
    >(
        self,
        consumer_token: &ConsumerToken,
        shared_region_ut: LocalCap<Untyped<GQueueSizeBits>>,
        local_vspace_scratch: &mut ScratchRegion<ScratchPages>,
        consumer_vspace: &mut VSpace,
        local_cnode: &LocalCap<LocalCNode>,
        umr_slots: LocalCNodeSlots<NumPages<GQueueSizeBits>>,
        shared_slots: LocalCNodeSlots<NumPages<GQueueSizeBits>>,
    ) -> Result<
        (
            Consumer3<role::Child, E, F, G, IRQ>,
            ProducerSetup<G, GLen, GQueueSizeBits>,
        ),
        MultiConsumerError,
    >
    where
        GLen: ArrayLength<Slot<G>>,
        GLen: IsGreater<U0, Output = True>,
        ScratchPages: IsGreaterOrEqual<NumPages<GQueueSizeBits>, Output = True>,

        // needed by temporarily_map_region
        GQueueSizeBits: IsGreaterOrEqual<PageBits>,
        GQueueSizeBits: Sub<PageBits>,
        <GQueueSizeBits as Sub<PageBits>>::Output: Unsigned,
        <GQueueSizeBits as Sub<PageBits>>::Output: _Pow,
        Pow<<GQueueSizeBits as Sub<PageBits>>::Output>:
            Unsigned + IsGreaterOrEqual<U1, Output = True>,

        // Needed by unmappedMemoryRegion::new
        Pow<<GQueueSizeBits as Sub<PageBits>>::Output>:
            IsLessOrEqual<KernelRetypeFanOutLimit, Output = True>,
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
            create_region_filled_with_array_queue::<ScratchPages, G, GLen, GQueueSizeBits>(
                shared_region_ut,
                local_vspace_scratch,
                consumer_vspace,
                &local_cnode,
                umr_slots,
                shared_slots,
            )?;

        let fresh_queue_badge = Badge::from((self.queues.1).0.inner << 1);
        let producer_setup: ProducerSetup<G, GLen, GQueueSizeBits> = ProducerSetup {
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
            _queue_length: PhantomData,
        };
        Ok((
            Consumer3 {
                irq_handler: self.irq_handler,
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
                            queue_len: GLen::USIZE,
                        },
                    ),
                ),
            },
            producer_setup,
        ))
    }
}

fn create_region_filled_with_array_queue<
    ScratchPages: Unsigned,
    T: Sized + Send + Sync,
    QLen: Unsigned,
    QSizeBits: Unsigned,
>(
    shared_region_ut: LocalCap<Untyped<QSizeBits>>,
    local_vspace_scratch: &mut ScratchRegion<ScratchPages>,
    consumer_vspace: &mut VSpace,
    local_cnode: &LocalCap<LocalCNode>,
    umr_slots: LocalCNodeSlots<NumPages<QSizeBits>>,
    shared_slots: LocalCNodeSlots<NumPages<QSizeBits>>,
) -> Result<
    (
        UnmappedMemoryRegion<QSizeBits, shared_status::Shared>,
        MappedMemoryRegion<QSizeBits, shared_status::Shared>,
    ),
    MultiConsumerError,
>
where
    QLen: ArrayLength<Slot<T>>,
    QLen: IsGreater<U0, Output = True>,
    ScratchPages: IsGreaterOrEqual<NumPages<QSizeBits>, Output = True>,

    // needed by temporarily_map_region
    QSizeBits: IsGreaterOrEqual<PageBits>,
    QSizeBits: Sub<PageBits>,
    <QSizeBits as Sub<PageBits>>::Output: Unsigned,
    <QSizeBits as Sub<PageBits>>::Output: _Pow,
    Pow<<QSizeBits as Sub<PageBits>>::Output>: Unsigned + IsGreaterOrEqual<U1, Output = True>,

    // Needed by unmappedMemoryRegion::new
    Pow<<QSizeBits as Sub<PageBits>>::Output>:
        IsLessOrEqual<KernelRetypeFanOutLimit, Output = True>,
{
    // Assert that there is enough space for the queue
    assert!(
        1 << QSizeBits::USIZE >= size_of::<ArrayQueue<T>>() + (QLen::USIZE * size_of::<Slot<T>>())
    );

    let mut region = UnmappedMemoryRegion::new(shared_region_ut, umr_slots)?;

    // Put some data in there. Specifically, an `ArrayQueue`.
    local_vspace_scratch.temporarily_map_region(&mut region, |mapped_region| unsafe {
        let aq_ptr = core::mem::transmute(mapped_region.vaddr());

        // Operate directly on a pointer to an uninitialized/zeroed pointer
        // in order to reduces odds of the full ArrayQueue instance
        // materializing all at once on the local stack (potentially blowing it)
        ArrayQueue::<T>::new_at_ptr(aq_ptr, QLen::USIZE, size_of::<ArrayQueue<T>>());
        core::mem::forget(aq_ptr);
    })?;

    let shared_region = region.to_shared();

    // put guard pages on either side of the shared region, so any overruns
    // become page faults instead of data corruption.
    consumer_vspace.skip_pages(1)?;
    let consumer_shared_region = consumer_vspace.map_shared_region(
        &shared_region,
        CapRights::RW,
        arch::vm_attributes::DEFAULT,
        shared_slots,
        local_cnode,
    )?;
    consumer_vspace.skip_pages(1)?;

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
impl<E: Sized + Sync + Send, IRQ: Unsigned> Consumer1<role::Local, E, IRQ>
where
    IRQ: IsLess<MaxIRQCount, Output = True>,
{
    pub fn capacity(&self) -> usize {
        self.queue.queue_len
    }

    pub fn poll(&mut self) -> Option<E> {
        let queue: &mut ArrayQueue<E> = unsafe { core::mem::transmute(self.queue.shared_queue) };

        if let Ok(e) = queue.pop() {
            Some(e)
        } else {
            None
        }
    }

    pub fn consume<State, WFn, EFn>(self, initial_state: State, waker_fn: WFn, queue_fn: EFn) -> !
    where
        WFn: Fn(State) -> State,
        EFn: Fn(E, State) -> State,
    {
        let mut sender_badge: usize = 0;
        let mut state = initial_state;
        let queue: &mut ArrayQueue<E> = unsafe { core::mem::transmute(self.queue.shared_queue) };
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
                    for _ in 0..queue.len().saturating_add(1) {
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

impl<E: Sized + Sync + Send, F: Sized + Sync + Send, IRQ: Unsigned> Consumer2<role::Local, E, F, IRQ>
where
    IRQ: IsLess<MaxIRQCount, Output = True>,
{
    pub fn capacity(&self) -> (usize, usize) {
        ((self.queues.0).1.queue_len, (self.queues.1).1.queue_len)
    }

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
        let queue_e: &mut ArrayQueue<E> = unsafe { core::mem::transmute(handle_e.shared_queue) };

        let (badge_f, handle_f) = self.queues.1;
        let queue_f: &mut ArrayQueue<F> = unsafe { core::mem::transmute(handle_f.shared_queue) };

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
                    for _ in 0..queue_e.len().saturating_add(1) {
                        if let Ok(e) = queue_e.pop() {
                            state = queue_e_fn(e, state);
                        } else {
                            break;
                        }
                    }
                }
                if badge_f.are_all_overlapping_bits_set(current_badge) {
                    for _ in 0..queue_f.len().saturating_add(1) {
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

impl<E: Sized + Sync + Send, F: Sized + Sync + Send, G: Sized + Sync + Send, IRQ: Unsigned>
    Consumer3<role::Local, E, F, G, IRQ>
where
    IRQ: IsLess<MaxIRQCount, Output = True>,
{
    pub fn capacity(&self) -> (usize, usize, usize) {
        (
            (self.queues.0).1.queue_len,
            (self.queues.1).1.queue_len,
            (self.queues.2).1.queue_len,
        )
    }

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
        let queue_e: &mut ArrayQueue<E> = unsafe { core::mem::transmute(handle_e.shared_queue) };

        let (badge_f, handle_f) = self.queues.1;
        let queue_f: &mut ArrayQueue<F> = unsafe { core::mem::transmute(handle_f.shared_queue) };

        let (badge_g, handle_g) = self.queues.2;
        let queue_g: &mut ArrayQueue<G> = unsafe { core::mem::transmute(handle_g.shared_queue) };

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
                    for _ in 0..queue_e.len().saturating_add(1) {
                        if let Ok(e) = queue_e.pop() {
                            state = queue_e_fn(e, state);
                        } else {
                            break;
                        }
                    }
                }
                if badge_f.are_all_overlapping_bits_set(current_badge) {
                    for _ in 0..queue_f.len().saturating_add(1) {
                        if let Ok(e) = queue_f.pop() {
                            state = queue_f_fn(e, state);
                        } else {
                            break;
                        }
                    }
                }
                if badge_g.are_all_overlapping_bits_set(current_badge) {
                    for _ in 0..queue_g.len().saturating_add(1) {
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

impl<T: Sized + Sync + Send, Role: CNodeRole> Producer<Role, T> {
    pub fn new<QSizeBits: Unsigned, QLen: Unsigned>(
        setup: &ProducerSetup<T, QLen, QSizeBits>,
        dest_slot: CNodeSlot<Role>,
        dest_vspace: &mut VSpace,
        local_cnode: &LocalCap<LocalCNode>,
        local_slots: LocalCNodeSlots<NumPages<QSizeBits>>,
    ) -> Result<Self, MultiConsumerError>
    where
        QLen: IsGreater<U0, Output = True>,
        QLen: ArrayLength<Slot<T>>,

        // needed for memoryregion
        QSizeBits: IsGreaterOrEqual<PageBits>,
        QSizeBits: Sub<PageBits>,
        <QSizeBits as Sub<PageBits>>::Output: Unsigned,
        <QSizeBits as Sub<PageBits>>::Output: _Pow,
        Pow<<QSizeBits as Sub<PageBits>>::Output>: Unsigned,
    {
        if setup.consumer_vspace_asid == dest_vspace.asid() {
            // To simplify reasoning about likely control flow patterns,
            // we presently disallow a consumer thread from producing to one
            // of its own ingest queues.
            return Err(MultiConsumerError::ProduceToOwnQueueForbidden);
        }
        let producer_shared_region = dest_vspace.map_shared_region(
            &setup.shared_region,
            CapRights::RW,
            arch::vm_attributes::DEFAULT,
            local_slots,
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
                queue_len: QLen::USIZE,
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

impl<T: Sized + Sync + Send> Producer<role::Local, T> {
    pub fn capacity(&self) -> usize {
        self.queue.queue_len
    }

    pub fn is_full(&self) -> bool {
        let queue: &ArrayQueue<T> = unsafe { core::mem::transmute(self.queue.shared_queue) };
        queue.is_full()
    }

    pub fn send(&self, t: T) -> Result<(), QueueFullError<T>> {
        let queue: &mut ArrayQueue<T> = unsafe { core::mem::transmute(self.queue.shared_queue) };
        queue.push(t)?;
        unsafe { seL4_Signal(self.notification.cptr) }
        Ok(())
    }
}
