use core::marker::PhantomData;
use core::ops::Sub;
use crate::userland::cap::AssignedPageDirectory;
use crate::userland::cap::Badge;
use crate::userland::paging::PageBytes;
use crate::userland::role;
use crate::userland::{
    CNodeRole, Cap, CapRights, ChildCNode, LocalCNode, LocalCap, MappedPageTable, Notification,
    PhantomCap, SeL4Error, UnmappedPage, Untyped, VSpace,
};
use cross_queue::PushError;
use cross_queue::{ArrayQueue, Slot};
use generic_array::ArrayLength;
use sel4_sys::{seL4_Signal, seL4_Wait};
use typenum::type_operators::Cmp;
use typenum::{Diff, Greater, IsGreater, Sub1, UTerm, Unsigned, B1, U0, U1, U12, U3, U4};

pub struct Consumer1<Role: CNodeRole, T: Sized + Sync + Send, QLen: Unsigned, P: QPtrType<T, QLen>>
where
    QLen: IsGreater<U0>,
    QLen: Cmp<U0, Output = Greater>,
    QLen: Cmp<UTerm>,
    QLen: ArrayLength<Slot<T>>,
{
    interrupt_badge: Badge,
    queue_badge: Badge,
    notification: Cap<Notification, Role>,
    queue: QueueHandle<T, Role, QLen, P>,
}

pub struct Producer<Role: CNodeRole, T: Sized + Sync + Send, QLen: Unsigned, P: QPtrType<T, QLen>>
where
    QLen: IsGreater<U0>,
    QLen: Cmp<U0, Output = Greater>,
    QLen: Cmp<UTerm>,
    QLen: ArrayLength<Slot<T>>,
{
    queue_badge: Badge,
    notification: Cap<Notification, Role>,
    queue: QueueHandle<T, Role, QLen, P>,
}

pub struct QueueHandle<T: Sized, Role: CNodeRole, QLen: Unsigned, P: QPtrType<T, QLen>>
where
    QLen: IsGreater<U0>,
    QLen: Cmp<U0, Output = Greater>,
    QLen: Cmp<UTerm>,
    QLen: ArrayLength<Slot<T>>,
{
    // Only valid in the VSpace context of a particular process
    shared_queue: <P as QPtrType<T, QLen>>::Type,
    _role: PhantomData<Role>,
}

/// QPtrType is a type-level function which converts a
/// pointer-as-usize to the actual pointer when the QueueHandle's
/// role changes from Child -> Local. This is to prevent unsafe
/// usage of its internal pointer to an `ArrayQueue`, which when
/// the `QueueHandle` is in `Child` mode, contains a `vaddr` which
/// is /not/ valid in that process's VSpace.
pub trait QPtrType<ElementType, QLen> {
    type Type;
    type _QLen = QLen;
    type _ElementType = ElementType;
}

impl<T: Sized, QLen: Unsigned> QPtrType<T, QLen> for role::Child
where
    QLen: IsGreater<U0>,
    QLen: ArrayLength<Slot<T>>,
{
    type Type = usize;
}

impl<T: Sized, QLen: Unsigned> QPtrType<T, QLen> for role::Local
where
    QLen: IsGreater<U0>,
    QLen: Cmp<U0, Output = Greater>,
    QLen: Cmp<UTerm>,
    QLen: ArrayLength<Slot<T>>,
{
    type Type = *mut ArrayQueue<T, QLen>;
}

pub enum DoubleDoorError {
    QueueTooBig,
    SeL4Error(SeL4Error),
}

impl From<SeL4Error> for DoubleDoorError {
    fn from(s: SeL4Error) -> Self {
        DoubleDoorError::SeL4Error(s)
    }
}

/// Wrapper around the necessary resources
/// to add a new producer to a given queue
pub struct ProducerSetup<T, QLen: Unsigned> {
    shared_page: LocalCap<UnmappedPage>,
    queue_badge: Badge,
    // User-concealed alias'ing happening here.
    // Don't mutate this Cap. Copying/minting is okay.
    notification: LocalCap<Notification>,
    _queue_element_type: PhantomData<T>,
    _queue_lenth: PhantomData<QLen>,
}

/// Wrapper around the necessary resources
/// to trigger a double door consumer's non-queue-reading
/// interrupt-oriented wakeup path.
pub struct WakerSetup {
    interrupt_badge: Badge,
    // User-concealed alias'ing happening here.
    // Don't mutate this Cap. Copying/minting is okay.
    notification: Cap<Notification, role::Local>,
}

/// A pattern for IPC with driver processes/threads
/// where there is a single (driver) consumer thread that is waiting
/// on a single notification.
/// There are two possible badge values for the notification, and
/// based on the badge, the consumer will do one of the following:
///
/// A) Execute a custom, interrupt-handling-specialized path.
/// B) Attempt to read from a shared memory queue. If an element is found, process it.
///
/// The alpha-path is intended to be bound to an interrupt notification,
/// but technically will work out of the box with any regular notification-sender
/// badged to match the A) path.
///
/// There may be many other threads producing to the shared memory queue.
/// A queue-producer thread requires:
/// * A capability to the notification, badged to correspond to the queue-path.
/// * The page(s) where the queue lives mapped into its VSpace
/// * A pointer to the shared memory queue valid in its VSpace.
///
/// There are two doors into the consumer thread. Do you pick door A or B?
///
/// let (consumer_params_member, queue_producer_setup, waker_setup,  ...leftovers) = double_door(...)
/// let (waker_params_member, ...leftovers) = Waker::new(waker_setup,waker_thread_cnode)
/// let (producer_params_member, ...leftovers) = Producer::new(queue_producer_setup, producer_thread_cnode, producer_thread_vspace)
pub fn double_door<
    'a,
    T: Sized + Send + Sync,
    QLen: Unsigned,
    P: QPtrType<T, QLen>,
    LocalCNodeFreeSlots: Unsigned,
    LocalPageDirFreeSlots: Unsigned,
    LocalPageTableFreeSlots: Unsigned,
    ConsumerCNodeFreeSlots: Unsigned,
    ConsumerPageDirFreeSlots: Unsigned,
    ConsumerPageTableFreeSlots: Unsigned,
    ConsumerFilledPageTableCount: Unsigned,
>(
    shared_page_ut: LocalCap<Untyped<U12>>,
    notification_ut: LocalCap<Untyped<U4>>,
    consumer_cnode: LocalCap<ChildCNode<ConsumerCNodeFreeSlots>>,
    consumer_vspace: VSpace<
        ConsumerPageDirFreeSlots,
        ConsumerPageTableFreeSlots,
        ConsumerFilledPageTableCount,
        role::Child,
    >,
    local_page_table: &'a mut LocalCap<MappedPageTable<LocalPageTableFreeSlots, role::Local>>,
    mut local_page_dir: &mut LocalCap<AssignedPageDirectory<LocalPageDirFreeSlots, role::Local>>,
    local_cnode: LocalCap<LocalCNode<LocalCNodeFreeSlots>>,
) -> Result<
    (
        Consumer1<role::Child, T, QLen, role::Child>,
        ProducerSetup<T, QLen>,
        WakerSetup,
        LocalCap<ChildCNode<Sub1<ConsumerCNodeFreeSlots>>>,
        VSpace<
            ConsumerPageDirFreeSlots,
            Sub1<ConsumerPageTableFreeSlots>,
            ConsumerFilledPageTableCount,
            role::Child,
        >,
        LocalCap<LocalCNode<Diff<LocalCNodeFreeSlots, U3>>>,
    ),
    DoubleDoorError,
>
where
    QLen: ArrayLength<Slot<T>>,
    QLen: IsGreater<U0>,
    QLen: Cmp<U0, Output = Greater>,
    QLen: Cmp<UTerm>,

    LocalCNodeFreeSlots: Sub<U3>,
    Diff<LocalCNodeFreeSlots, U3>: Unsigned,

    LocalPageTableFreeSlots: Sub<B1>,
    Sub1<LocalPageTableFreeSlots>: Unsigned,

    ConsumerCNodeFreeSlots: Sub<B1>,
    Sub1<ConsumerCNodeFreeSlots>: Unsigned,

    ConsumerPageTableFreeSlots: Sub<B1>,
    Sub1<ConsumerPageTableFreeSlots>: Unsigned,

    ConsumerFilledPageTableCount: ArrayLength<LocalCap<MappedPageTable<U0, role::Child>>>,
{
    let queue_size = core::mem::size_of::<ArrayQueue<T, QLen>>();
    if queue_size > PageBytes::USIZE {
        return Err(DoubleDoorError::QueueTooBig);
    }
    let (local_cnode, remainder_local_cnode) = local_cnode.reserve_region::<U3>();

    let (shared_page, local_cnode) = shared_page_ut.retype_local::<_, UnmappedPage>(local_cnode)?;

    // Put some data in there. Specifically, an `ArrayQueue`.
    let (_, shared_page) =
        local_page_table.temporarily_map_page(shared_page, &mut local_page_dir, |mapped_page| {
            unsafe {
                let aq_ptr = core::mem::transmute::<usize, *mut ArrayQueue<T, QLen>>(
                    mapped_page.cap_data.vaddr,
                );
                // TODO - consider making an alternative constructor
                // that operates directly on a pointer to an uninitialized/zeroed pointer
                // in order to avoid letting the full ArrayQueue instance
                // materialize all at once on the local stack (potentially blowing it)
                *aq_ptr = ArrayQueue::<T, QLen>::new();
            }
        })?;

    let (consumer_shared_page, local_cnode) =
        shared_page.copy_inside_cnode(local_cnode, CapRights::RW)?;
    let (consumer_shared_page, consumer_vspace) = consumer_vspace.map_page(consumer_shared_page)?;

    let (local_notification, local_cnode) =
        notification_ut.retype_local::<_, Notification>(local_cnode)?;
    let (consumer_notification, consumer_cnode) = local_notification.mint(
        &local_cnode,
        consumer_cnode,
        CapRights::RWG,
        Badge::from(0x00), // Only for Wait'ing, no need to set badge bits
    )?;
    let interrupt_badge = Badge::from(1 << 0);
    let queue_badge = Badge::from(1 << 1);

    let producer_setup: ProducerSetup<T, QLen> = ProducerSetup {
        shared_page,
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
    let waker_setup = WakerSetup {
        interrupt_badge: interrupt_badge,
        notification: local_notification,
    };
    Ok((
        Consumer1 {
            interrupt_badge,
            queue_badge,
            notification: consumer_notification,
            queue: QueueHandle {
                shared_queue: consumer_shared_page.cap_data.vaddr,
                _role: PhantomData,
            },
        },
        producer_setup,
        waker_setup,
        consumer_cnode,
        consumer_vspace,
        remainder_local_cnode,
    ))
}

pub struct Waker<Role: CNodeRole> {
    notification: Cap<Notification, Role>,
}
impl Waker<role::Child> {
    pub fn new<ChildCNodeSlots: Unsigned, LocalCNodeSlots: Unsigned>(
        setup: &WakerSetup,
        child_cnode: LocalCap<ChildCNode<ChildCNodeSlots>>,
        local_cnode: &LocalCap<LocalCNode<LocalCNodeSlots>>,
    ) -> Result<(Self, LocalCap<ChildCNode<Sub1<ChildCNodeSlots>>>), SeL4Error>
    where
        ChildCNodeSlots: Sub<B1>,
        Sub1<ChildCNodeSlots>: Unsigned,
    {
        let (notification, child_cnode) = setup.notification.mint(
            local_cnode,
            child_cnode,
            CapRights::RWG,
            setup.interrupt_badge,
        )?;
        Ok((Waker { notification }, child_cnode))
    }
}

impl Waker<role::Local> {
    pub fn send_wakeup_signal(&self) {
        unsafe {
            seL4_Signal(self.notification.cptr);
        }
    }
}

impl<E: Sized + Sync + Send, QLen: Unsigned> Consumer1<role::Local, E, QLen, role::Local>
where
    QLen: IsGreater<U0>,
    QLen: ArrayLength<Slot<E>>,
    QLen: Cmp<U0, Output = Greater>,
    QLen: Cmp<UTerm>,
{
    pub fn consume<State, WFn, EFn>(self, initial_state: State, waker_fn: WFn, queue_fn: EFn) -> !
    where
        WFn: Fn(State) -> State,
        EFn: Fn(E, State) -> State,
    {
        let mut sender_badge: usize = 0;
        let mut state = initial_state;
        let queue: &mut ArrayQueue<_, _> = unsafe { &mut *self.queue.shared_queue };
        loop {
            unsafe {
                seL4_Wait(self.notification.cptr, &mut sender_badge as *mut usize);
                let current_badge = Badge::from(sender_badge);
                if self
                    .interrupt_badge
                    .are_all_overlapping_bits_set(current_badge)
                {
                    state = waker_fn(state);
                }
                if self.queue_badge.are_all_overlapping_bits_set(current_badge) {
                    if let Ok(e) = queue.pop() {
                        state = queue_fn(e, state);
                    }
                }
            }
        }
    }
}
impl<T: Sized + Sync + Send, QLen: Unsigned> Producer<role::Child, T, QLen, role::Child>
where
    QLen: IsGreater<U0>,
    QLen: ArrayLength<Slot<T>>,
    QLen: Cmp<U0, Output = Greater>,
    QLen: Cmp<UTerm>,
{
    pub fn new<
        ChildCNodeSlots: Unsigned,
        LocalCNodeSlots: Unsigned,
        ChildPageDirSlots: Unsigned,
        ChildPageTableSlots: Unsigned,
        ChildFilledPageTableCount: Unsigned,
    >(
        setup: &ProducerSetup<T, QLen>,
        child_cnode: LocalCap<ChildCNode<ChildCNodeSlots>>,
        child_vspace: VSpace<
            ChildPageDirSlots,
            ChildPageTableSlots,
            ChildFilledPageTableCount,
            role::Child,
        >,
        local_cnode: LocalCap<LocalCNode<LocalCNodeSlots>>,
    ) -> Result<
        (
            Self,
            LocalCap<ChildCNode<Sub1<ChildCNodeSlots>>>,
            VSpace<
                ChildPageDirSlots,
                Sub1<ChildPageTableSlots>,
                ChildFilledPageTableCount,
                role::Child,
            >,
            LocalCap<LocalCNode<Sub1<LocalCNodeSlots>>>,
        ),
        SeL4Error,
    >
    where
        LocalCNodeSlots: Sub<B1>,
        Sub1<LocalCNodeSlots>: Unsigned,

        ChildCNodeSlots: Sub<B1>,
        Sub1<ChildCNodeSlots>: Unsigned,

        ChildCNodeSlots: Sub<B1>,
        Sub1<ChildCNodeSlots>: Unsigned,

        ChildPageTableSlots: Sub<B1>,
        Sub1<ChildPageTableSlots>: Unsigned,

        ChildFilledPageTableCount: ArrayLength<LocalCap<MappedPageTable<U0, role::Child>>>,
    {
        let (producer_shared_page, local_cnode) = setup
            .shared_page
            .copy_inside_cnode(local_cnode, CapRights::RW)?;
        let (producer_shared_page, child_vspace) = child_vspace.map_page(producer_shared_page)?;
        let (notification, child_cnode) = setup.notification.mint(
            &local_cnode,
            child_cnode,
            CapRights::RWG,
            setup.queue_badge,
        )?;
        Ok((
            Producer {
                queue_badge: setup.queue_badge,
                notification,
                queue: QueueHandle {
                    shared_queue: producer_shared_page.cap_data.vaddr,
                    _role: PhantomData,
                },
            },
            child_cnode,
            child_vspace,
            local_cnode,
        ))
    }
}

impl<T: Sized + Sync + Send, QLen: Unsigned> Producer<role::Local, T, QLen, role::Local>
where
    QLen: IsGreater<U0>,
    QLen: ArrayLength<Slot<T>>,
    QLen: Cmp<U0, Output = Greater>,
    QLen: Cmp<UTerm>,
{
    fn send(&self, t: T) -> Result<(), PushError<T>> {
        let queue: &mut ArrayQueue<_, _> = unsafe { &mut *self.queue.shared_queue };
        queue.push(t)?;
        unsafe { seL4_Signal(self.notification.cptr) }
        Ok(())
    }
}
