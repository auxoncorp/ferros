/// A pattern for async IPC with driver processes/threads
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
use core::marker::PhantomData;
use core::ops::Sub;
use crate::userland::cap::AssignedPageDirectory;
use crate::userland::cap::Badge;
use crate::userland::paging::PageBytes;
use crate::userland::role;
use crate::userland::{
    CNodeRole, Cap, CapRights, ChildCNode, LocalCNode, LocalCap, MappedPage, MappedPageTable,
    Notification, PhantomCap, SeL4Error, UnmappedPage, Untyped, VSpace,
};
use cross_queue::PushError;
use cross_queue::{ArrayQueue, Slot};
use generic_array::ArrayLength;
use sel4_sys::{seL4_Signal, seL4_Wait};
use typenum::{Diff, IsGreater, Sub1, True, Unsigned, B1, U0, U12, U2, U3, U4};

pub struct Consumer1<Role: CNodeRole, T: Sized + Sync + Send, QLen: Unsigned>
where
    QLen: IsGreater<U0, Output = True>,
    QLen: ArrayLength<Slot<T>>,
{
    interrupt_badge: Badge,
    notification: Cap<Notification, Role>,
    queue_badge: Badge,
    queue: QueueHandle<T, Role, QLen>,
}

pub struct Consumer2<Role: CNodeRole, E, ESize: Unsigned, F, FSize: Unsigned>
where
    ESize: IsGreater<U0, Output = True>,
    ESize: ArrayLength<Slot<E>>,
    FSize: IsGreater<U0, Output = True>,
    FSize: ArrayLength<Slot<F>>,
{
    interrupt_badge: Badge,
    notification: Cap<Notification, Role>,
    queues: (
        (Badge, QueueHandle<E, Role, ESize>),
        (Badge, QueueHandle<F, Role, FSize>),
    ),
}

pub struct Consumer3<Role: CNodeRole, E, ESize: Unsigned, F, FSize: Unsigned, G, GSize: Unsigned>
where
    ESize: IsGreater<U0, Output = True>,
    ESize: ArrayLength<Slot<E>>,
    FSize: IsGreater<U0, Output = True>,
    FSize: ArrayLength<Slot<F>>,
    GSize: IsGreater<U0, Output = True>,
    GSize: ArrayLength<Slot<G>>,
{
    interrupt_badge: Badge,
    notification: Cap<Notification, Role>,
    queues: (
        (Badge, QueueHandle<E, Role, ESize>),
        (Badge, QueueHandle<F, Role, FSize>),
        (Badge, QueueHandle<G, Role, GSize>),
    ),
}

pub struct Producer<Role: CNodeRole, T: Sized + Sync + Send, QLen: Unsigned>
where
    QLen: IsGreater<U0, Output = True>,
    QLen: ArrayLength<Slot<T>>,
{
    notification: Cap<Notification, Role>,
    queue: QueueHandle<T, Role, QLen>,
}

pub struct QueueHandle<T: Sized, Role: CNodeRole, QLen: Unsigned>
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

#[derive(Debug)]
pub enum MultiConsumerError {
    QueueTooBig,
    SeL4Error(SeL4Error),
}

impl From<SeL4Error> for MultiConsumerError {
    fn from(s: SeL4Error) -> Self {
        MultiConsumerError::SeL4Error(s)
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

impl<E: Sized + Sync + Send, ELen: Unsigned> Consumer1<role::Child, E, ELen>
where
    ELen: IsGreater<U0, Output = True>,
    ELen: ArrayLength<Slot<E>>,
{
    pub fn new<
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
        local_page_table: &mut LocalCap<MappedPageTable<LocalPageTableFreeSlots, role::Local>>,
        mut local_page_dir: &mut LocalCap<
            AssignedPageDirectory<LocalPageDirFreeSlots, role::Local>,
        >,
        local_cnode: LocalCap<LocalCNode<LocalCNodeFreeSlots>>,
    ) -> Result<
        (
            Consumer1<role::Child, E, ELen>,
            ProducerSetup<E, ELen>,
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
        MultiConsumerError,
    >
    where
        ELen: ArrayLength<Slot<E>>,
        ELen: IsGreater<U0, Output = True>,

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
        let queue_size = core::mem::size_of::<ArrayQueue<E, ELen>>();
        if queue_size > PageBytes::USIZE {
            return Err(MultiConsumerError::QueueTooBig);
        }
        let (local_cnode, remainder_local_cnode) = local_cnode.reserve_region::<U3>();
        let (shared_page, consumer_shared_page, consumer_vspace, local_cnode) =
            create_page_filled_with_array_queue::<E, ELen, _, _, _, _, _, _>(
                shared_page_ut,
                consumer_vspace,
                local_page_table,
                local_page_dir,
                local_cnode,
            )?;

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

        let producer_setup: ProducerSetup<E, ELen> = ProducerSetup {
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
                    _t: PhantomData,
                    _queue_len: PhantomData,
                },
            },
            producer_setup,
            waker_setup,
            consumer_cnode,
            consumer_vspace,
            remainder_local_cnode,
        ))
    }

    pub fn add_queue<
        F: Sized + Send + Sync,
        FLen: Unsigned,
        LocalCNodeFreeSlots: Unsigned,
        LocalPageDirFreeSlots: Unsigned,
        LocalPageTableFreeSlots: Unsigned,
        ConsumerPageDirFreeSlots: Unsigned,
        ConsumerPageTableFreeSlots: Unsigned,
        ConsumerFilledPageTableCount: Unsigned,
    >(
        self,
        producer_setup: &ProducerSetup<E, ELen>,
        shared_page_ut: LocalCap<Untyped<U12>>,
        consumer_vspace: VSpace<
            ConsumerPageDirFreeSlots,
            ConsumerPageTableFreeSlots,
            ConsumerFilledPageTableCount,
            role::Child,
        >,
        local_page_table: &mut LocalCap<MappedPageTable<LocalPageTableFreeSlots, role::Local>>,
        mut local_page_dir: &mut LocalCap<
            AssignedPageDirectory<LocalPageDirFreeSlots, role::Local>,
        >,
        local_cnode: LocalCap<LocalCNode<LocalCNodeFreeSlots>>,
    ) -> Result<
        (
            Consumer2<role::Child, E, ELen, F, FLen>,
            ProducerSetup<F, FLen>,
            VSpace<
                ConsumerPageDirFreeSlots,
                Sub1<ConsumerPageTableFreeSlots>,
                ConsumerFilledPageTableCount,
                role::Child,
            >,
            LocalCap<LocalCNode<Diff<LocalCNodeFreeSlots, U2>>>,
        ),
        MultiConsumerError,
    >
    where
        FLen: ArrayLength<Slot<F>>,
        FLen: IsGreater<U0, Output = True>,

        LocalCNodeFreeSlots: Sub<U2>,
        Diff<LocalCNodeFreeSlots, U2>: Unsigned,

        LocalPageTableFreeSlots: Sub<B1>,
        Sub1<LocalPageTableFreeSlots>: Unsigned,

        ConsumerPageTableFreeSlots: Sub<B1>,
        Sub1<ConsumerPageTableFreeSlots>: Unsigned,

        ConsumerFilledPageTableCount: ArrayLength<LocalCap<MappedPageTable<U0, role::Child>>>,
    {
        let (shared_page, consumer_shared_page, consumer_vspace, remainder_local_cnode) =
            create_page_filled_with_array_queue::<F, FLen, _, _, _, _, _, _>(
                shared_page_ut,
                consumer_vspace,
                local_page_table,
                local_page_dir,
                local_cnode,
            )?;

        let fresh_queue_badge = Badge::from(self.queue_badge.inner << 1);
        let producer_setup: ProducerSetup<F, FLen> = ProducerSetup {
            shared_page,
            queue_badge: fresh_queue_badge,
            // Construct a user-inaccessible copy of the local notification
            // purely for use in producing child-cnode-residing copies.
            notification: Cap {
                cptr: producer_setup.notification.cptr,
                cap_data: PhantomCap::phantom_instance(),
                _role: PhantomData,
            },
            _queue_element_type: PhantomData,
            _queue_lenth: PhantomData,
        };
        Ok((
            Consumer2 {
                interrupt_badge: self.interrupt_badge,
                notification: self.notification,
                queues: (
                    (self.queue_badge, self.queue),
                    (
                        fresh_queue_badge,
                        QueueHandle {
                            shared_queue: consumer_shared_page.cap_data.vaddr,
                            _role: PhantomData,
                            _t: PhantomData,
                            _queue_len: PhantomData,
                        },
                    ),
                ),
            },
            producer_setup,
            consumer_vspace,
            remainder_local_cnode,
        ))
    }
}

impl<E: Sized + Sync + Send, ELen: Unsigned, F: Sized + Sync + Send, FLen: Unsigned>
    Consumer2<role::Child, E, ELen, F, FLen>
where
    ELen: IsGreater<U0, Output = True>,
    ELen: ArrayLength<Slot<E>>,
    FLen: IsGreater<U0, Output = True>,
    FLen: ArrayLength<Slot<F>>,
{
    pub fn add_queue<
        G: Sized + Send + Sync,
        GLen: Unsigned,
        LocalCNodeFreeSlots: Unsigned,
        LocalPageDirFreeSlots: Unsigned,
        LocalPageTableFreeSlots: Unsigned,
        ConsumerPageDirFreeSlots: Unsigned,
        ConsumerPageTableFreeSlots: Unsigned,
        ConsumerFilledPageTableCount: Unsigned,
    >(
        self,
        producer_setup: &ProducerSetup<E, ELen>,
        shared_page_ut: LocalCap<Untyped<U12>>,
        consumer_vspace: VSpace<
            ConsumerPageDirFreeSlots,
            ConsumerPageTableFreeSlots,
            ConsumerFilledPageTableCount,
            role::Child,
        >,
        local_page_table: &mut LocalCap<MappedPageTable<LocalPageTableFreeSlots, role::Local>>,
        local_page_dir: &mut LocalCap<AssignedPageDirectory<LocalPageDirFreeSlots, role::Local>>,
        local_cnode: LocalCap<LocalCNode<LocalCNodeFreeSlots>>,
    ) -> Result<
        (
            Consumer3<role::Child, E, ELen, F, FLen, G, GLen>,
            ProducerSetup<F, FLen>,
            VSpace<
                ConsumerPageDirFreeSlots,
                Sub1<ConsumerPageTableFreeSlots>,
                ConsumerFilledPageTableCount,
                role::Child,
            >,
            LocalCap<LocalCNode<Diff<LocalCNodeFreeSlots, U2>>>,
        ),
        MultiConsumerError,
    >
    where
        FLen: ArrayLength<Slot<F>>,
        FLen: IsGreater<U0, Output = True>,
        GLen: ArrayLength<Slot<G>>,
        GLen: IsGreater<U0, Output = True>,

        LocalCNodeFreeSlots: Sub<U2>,
        Diff<LocalCNodeFreeSlots, U2>: Unsigned,

        LocalPageTableFreeSlots: Sub<B1>,
        Sub1<LocalPageTableFreeSlots>: Unsigned,

        ConsumerPageTableFreeSlots: Sub<B1>,
        Sub1<ConsumerPageTableFreeSlots>: Unsigned,

        ConsumerFilledPageTableCount: ArrayLength<LocalCap<MappedPageTable<U0, role::Child>>>,
    {
        let (shared_page, consumer_shared_page, consumer_vspace, remainder_local_cnode) =
            create_page_filled_with_array_queue::<F, FLen, _, _, _, _, _, _>(
                shared_page_ut,
                consumer_vspace,
                local_page_table,
                local_page_dir,
                local_cnode,
            )?;

        let fresh_queue_badge = Badge::from((self.queues.1).0.inner << 1);
        let producer_setup: ProducerSetup<F, FLen> = ProducerSetup {
            shared_page,
            queue_badge: fresh_queue_badge,
            // Construct a user-inaccessible copy of the local notification
            // purely for use in producing child-cnode-residing copies.
            notification: Cap {
                cptr: producer_setup.notification.cptr,
                cap_data: PhantomCap::phantom_instance(),
                _role: PhantomData,
            },
            _queue_element_type: PhantomData,
            _queue_lenth: PhantomData,
        };
        Ok((
            Consumer3 {
                interrupt_badge: self.interrupt_badge,
                notification: self.notification,
                queues: (
                    self.queues.0,
                    self.queues.1,
                    (
                        fresh_queue_badge,
                        QueueHandle {
                            shared_queue: consumer_shared_page.cap_data.vaddr,
                            _role: PhantomData,
                            _t: PhantomData,
                            _queue_len: PhantomData,
                        },
                    ),
                ),
            },
            producer_setup,
            consumer_vspace,
            remainder_local_cnode,
        ))
    }
}

fn create_page_filled_with_array_queue<
    T: Sized + Send + Sync,
    QLen: Unsigned,
    LocalCNodeFreeSlots: Unsigned,
    LocalPageDirFreeSlots: Unsigned,
    LocalPageTableFreeSlots: Unsigned,
    ConsumerPageDirFreeSlots: Unsigned,
    ConsumerPageTableFreeSlots: Unsigned,
    ConsumerFilledPageTableCount: Unsigned,
>(
    shared_page_ut: LocalCap<Untyped<U12>>,
    consumer_vspace: VSpace<
        ConsumerPageDirFreeSlots,
        ConsumerPageTableFreeSlots,
        ConsumerFilledPageTableCount,
        role::Child,
    >,
    local_page_table: &mut LocalCap<MappedPageTable<LocalPageTableFreeSlots, role::Local>>,
    mut local_page_dir: &mut LocalCap<AssignedPageDirectory<LocalPageDirFreeSlots, role::Local>>,
    local_cnode: LocalCap<LocalCNode<LocalCNodeFreeSlots>>,
) -> Result<
    (
        LocalCap<UnmappedPage>,
        LocalCap<MappedPage<role::Child>>,
        VSpace<
            ConsumerPageDirFreeSlots,
            Sub1<ConsumerPageTableFreeSlots>,
            ConsumerFilledPageTableCount,
            role::Child,
        >,
        LocalCap<LocalCNode<Diff<LocalCNodeFreeSlots, U2>>>,
    ),
    MultiConsumerError,
>
where
    QLen: ArrayLength<Slot<T>>,
    QLen: IsGreater<U0, Output = True>,

    LocalCNodeFreeSlots: Sub<U2>,
    Diff<LocalCNodeFreeSlots, U2>: Unsigned,

    LocalPageTableFreeSlots: Sub<B1>,
    Sub1<LocalPageTableFreeSlots>: Unsigned,

    ConsumerPageTableFreeSlots: Sub<B1>,
    Sub1<ConsumerPageTableFreeSlots>: Unsigned,

    ConsumerFilledPageTableCount: ArrayLength<LocalCap<MappedPageTable<U0, role::Child>>>,
{
    let (local_cnode, remainder_local_cnode) = local_cnode.reserve_region::<U2>();
    let queue_size = core::mem::size_of::<ArrayQueue<T, QLen>>();
    if queue_size > PageBytes::USIZE {
        return Err(MultiConsumerError::QueueTooBig);
    }
    let (shared_page, local_cnode) = shared_page_ut.retype_local::<_, UnmappedPage>(local_cnode)?;
    // Put some data in there. Specifically, an `ArrayQueue`.
    let (_, shared_page) =
        local_page_table.temporarily_map_page(shared_page, &mut local_page_dir, |mapped_page| {
            unsafe {
                let aq_ptr = core::mem::transmute::<usize, *mut ArrayQueue<T, QLen>>(
                    mapped_page.cap_data.vaddr,
                );
                // Operate directly on a pointer to an uninitialized/zeroed pointer
                // in order to reduces odds of the full ArrayQueue instance
                // materializing all at once on the local stack (potentially blowing it)
                ArrayQueue::<T, QLen>::new_at_ptr(aq_ptr);
                core::mem::forget(aq_ptr);
            }
        })?;
    let (consumer_shared_page, _local_cnode) =
        shared_page.copy_inside_cnode(local_cnode, CapRights::RW)?;
    let (consumer_shared_page, consumer_vspace) = consumer_vspace.map_page(consumer_shared_page)?;
    Ok((
        shared_page,
        consumer_shared_page,
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
                notification,
                queue: QueueHandle {
                    shared_queue: producer_shared_page.cap_data.vaddr,
                    _role: PhantomData,
                    _t: PhantomData,
                    _queue_len: PhantomData,
                },
            },
            child_cnode,
            child_vspace,
            local_cnode,
        ))
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
