use core::marker::PhantomData;
use core::ops::Sub;
use crate::userland::paging::PageBytes;
use crate::userland::{
    role, Badge, CNodeRole, Cap, CapRights, ChildCNode, IPCError, LocalCNode, LocalCap,
    MappedPageTable, Notification, UnmappedPage, Untyped, VSpace,
};
use generic_array::ArrayLength;
use sel4_sys::{seL4_Signal, seL4_Wait};
use typenum::operator_aliases::{Diff, Sub1};
use typenum::{Unsigned, B1, U0, U12, U2, U4, U5};

pub mod lowlock {
    use super::*;
    use core::cell::{Cell, UnsafeCell};
    use core::fmt;
    use core::mem;
    use core::ops::{Deref, DerefMut};
    use core::ptr;
    use core::sync::atomic::{self, AtomicUsize, Ordering};

    // Per Consumer:
    // Create a new Notification associate with a type managing badge-bit capacity
    // one copy of the capability to that notification in the CSpace of the consumer thread (with read permissions)
    // one path for dealing with the "interrupt" based wakeup
    //
    // Per Queue:
    // backing shared page(s) per queue
    // a single bit index from the badge bit-space per queue
    // an associated element type
    // access to the local (parent?) VSpace in order to do local mapping for setup? OR we do this in consumer
    //
    // Per Producer:
    // a copy of the notification capability with write permissions in the CSpace of the producer thread
    // a mapping of the queue backing pages for the relevant queue

    pub fn setup_consumer() -> Consumer

    pub struct Consumer1<E, Role: CNodeRole> {
        notification: Cap<Notification, Role>,
        queues: QueueHandle<E, Role>,
    }

    pub struct Consumer2<E, F> {
        queues: (QueueHandle<E, Role>, QueueHandle<F, Role>)
    }

    pub struct Consumer3<E, F, G> {
        queues: (QueueHandle<E, Role>, QueueHandle<F, Role>, QueueHandle<G, Role>)
    }

    impl Consumer3<E, F, G> {
        fn consume_loop<X, Y, Z, IH, State>(
            self,
            x:X, y:Y, z:Z,
            interrupt_handler: IH,
            initial_state:State)
        where
        // TODO - state piping
            X: Fn(E, State) -> State, // TODO - errors whaaaaat
            Y: Fn(F, State) -> State,
            Z: Fn(G, State) -> State,
            IH: Fn(State) -> State,
        {
            unimplemented!()
        }
    }

    #[derive(Debug)]
    pub struct QueueHandle<T: Sized, Role: CNodeRole> {
        // Only valid in the VSpace context of a particular process
        shared_queue: *mut ArrayQueue<T>,
        _role: PhantomData<Role>,
    }

    // ---- Cribbed from crossbeam -----

    /// A slot in a queue.
    struct Slot<T> {
        /// The current stamp.
        ///
        /// If the stamp equals the tail, this node will be next written to. If it equals the head,
        /// this node will be next read from.
        stamp: AtomicUsize,

        /// The value in this slot.
        value: UnsafeCell<T>,
    }
    pub struct ArrayQueue<T> {
        /// The head of the queue.
        ///
        /// This value is a "stamp" consisting of an index into the buffer and a lap, but packed into a
        /// single `usize`. The lower bits represent the index, while the upper bits represent the lap.
        ///
        /// Elements are popped from the head of the queue.
        head: CachePadded<AtomicUsize>,

        /// The tail of the queue.
        ///
        /// This value is a "stamp" consisting of an index into the buffer and a lap, but packed into a
        /// single `usize`. The lower bits represent the index, while the upper bits represent the lap.
        ///
        /// Elements are pushed into the tail of the queue.
        tail: CachePadded<AtomicUsize>,

        /// The buffer holding slots.
        buffer: *mut Slot<T>,

        /// The queue capacity.
        cap: usize,

        /// A stamp with the value of `{ lap: 1, index: 0 }`.
        one_lap: usize,

        /// Indicates that dropping an `ArrayQueue<T>` may drop elements of type `T`.
        _marker: PhantomData<T>,
    }

    unsafe impl<T: Send> Sync for ArrayQueue<T> {}
    unsafe impl<T: Send> Send for ArrayQueue<T> {}
    #[repr(align(64))]
    struct CachePadded<T> {
        value: T,
    }
    unsafe impl<T: Send> Send for CachePadded<T> {}
    unsafe impl<T: Sync> Sync for CachePadded<T> {}
    impl<T> CachePadded<T> {
        /// Pads and aligns a value to the length of a cache line.
        ///
        /// # Examples
        ///
        /// ```
        /// use crossbeam_utils::CachePadded;
        ///
        /// let padded_value = CachePadded::new(1);
        /// ```
        pub fn new(t: T) -> CachePadded<T> {
            CachePadded::<T> { value: t }
        }

        /// Returns the value value.
        ///
        /// # Examples
        ///
        /// ```
        /// use crossbeam_utils::CachePadded;
        ///
        /// let padded_value = CachePadded::new(7);
        /// let value = padded_value.into_inner();
        /// assert_eq!(value, 7);
        /// ```
        pub fn into_inner(self) -> T {
            self.value
        }
    }

    impl<T> Deref for CachePadded<T> {
        type Target = T;

        fn deref(&self) -> &T {
            &self.value
        }
    }

    impl<T> DerefMut for CachePadded<T> {
        fn deref_mut(&mut self) -> &mut T {
            &mut self.value
        }
    }

    impl<T> ArrayQueue<T> {
        /// Creates a new bounded queue with the given capacity.
        ///
        /// # Panics
        ///
        /// Panics if the capacity is zero.
        ///
        /// # Examples
        ///
        /// ```
        /// use crossbeam_queue::ArrayQueue;
        ///
        /// let q = ArrayQueue::<i32>::new(100);
        /// ```
        pub fn new(cap: usize) -> ArrayQueue<T> {
            assert!(cap > 0, "capacity must be non-zero");

            // Head is initialized to `{ lap: 0, index: 0 }`.
            // Tail is initialized to `{ lap: 0, index: 0 }`.
            let head = 0;
            let tail = 0;

            // TODO - replace with a (generic-sized) array
            // Allocate a buffer of `cap` slots.
            let buffer = {
                let mut v = Vec::<Slot<T>>::with_capacity(cap);
                let ptr = v.as_mut_ptr();
                mem::forget(v);
                ptr
            };

            // Initialize stamps in the slots.
            for i in 0..cap {
                unsafe {
                    // Set the stamp to `{ lap: 0, index: i }`.
                    let slot = buffer.add(i);
                    ptr::write(&mut (*slot).stamp, AtomicUsize::new(i));
                }
            }

            // One lap is the smallest power of two greater than `cap`.
            let one_lap = (cap + 1).next_power_of_two();

            ArrayQueue {
                buffer,
                cap,
                one_lap,
                head: CachePadded::new(AtomicUsize::new(head)),
                tail: CachePadded::new(AtomicUsize::new(tail)),
                _marker: PhantomData,
            }
        }

        /// Attempts to push an element into the queue.
        ///
        /// If the queue is full, the element is returned back as an error.
        ///
        /// # Examples
        ///
        /// ```
        /// use crossbeam_queue::{ArrayQueue, PushError};
        ///
        /// let q = ArrayQueue::new(1);
        ///
        /// assert_eq!(q.push(10), Ok(()));
        /// assert_eq!(q.push(20), Err(PushError(20)));
        /// ```
        pub fn push(&self, value: T) -> Result<(), PushError<T>> {
            let backoff = Backoff::new();
            let mut tail = self.tail.load(Ordering::Relaxed);

            loop {
                // Deconstruct the tail.
                let index = tail & (self.one_lap - 1);
                let lap = tail & !(self.one_lap - 1);

                // Inspect the corresponding slot.
                let slot = unsafe { &*self.buffer.add(index) };
                let stamp = slot.stamp.load(Ordering::Acquire);

                // If the tail and the stamp match, we may attempt to push.
                if tail == stamp {
                    let new_tail = if index + 1 < self.cap {
                        // Same lap, incremented index.
                        // Set to `{ lap: lap, index: index + 1 }`.
                        tail + 1
                    } else {
                        // One lap forward, index wraps around to zero.
                        // Set to `{ lap: lap.wrapping_add(1), index: 0 }`.
                        lap.wrapping_add(self.one_lap)
                    };

                    // Try moving the tail.
                    match self.tail.compare_exchange_weak(
                        tail,
                        new_tail,
                        Ordering::SeqCst,
                        Ordering::Relaxed,
                    ) {
                        Ok(_) => {
                            // Write the value into the slot and update the stamp.
                            unsafe {
                                slot.value.get().write(value);
                            }
                            slot.stamp.store(tail + 1, Ordering::Release);
                            return Ok(());
                        }
                        Err(t) => {
                            tail = t;
                            backoff.spin();
                        }
                    }
                } else if stamp.wrapping_add(self.one_lap) == tail + 1 {
                    atomic::fence(Ordering::SeqCst);
                    let head = self.head.load(Ordering::Relaxed);

                    // If the head lags one lap behind the tail as well...
                    if head.wrapping_add(self.one_lap) == tail {
                        // ...then the queue is full.
                        return Err(PushError(value));
                    }

                    backoff.spin();
                    tail = self.tail.load(Ordering::Relaxed);
                } else {
                    // Snooze because we need to wait for the stamp to get updated.
                    backoff.snooze();
                    tail = self.tail.load(Ordering::Relaxed);
                }
            }
        }

        /// Attempts to pop an element from the queue.
        ///
        /// If the queue is empty, an error is returned.
        ///
        /// # Examples
        ///
        /// ```
        /// use crossbeam_queue::{ArrayQueue, PopError};
        ///
        /// let q = ArrayQueue::new(1);
        /// assert_eq!(q.push(10), Ok(()));
        ///
        /// assert_eq!(q.pop(), Ok(10));
        /// assert_eq!(q.pop(), Err(PopError));
        /// ```
        pub fn pop(&self) -> Result<T, PopError> {
            let backoff = Backoff::new();
            let mut head = self.head.load(Ordering::Relaxed);

            loop {
                // Deconstruct the head.
                let index = head & (self.one_lap - 1);
                let lap = head & !(self.one_lap - 1);

                // Inspect the corresponding slot.
                let slot = unsafe { &*self.buffer.add(index) };
                let stamp = slot.stamp.load(Ordering::Acquire);

                // If the the stamp is ahead of the head by 1, we may attempt to pop.
                if head + 1 == stamp {
                    let new = if index + 1 < self.cap {
                        // Same lap, incremented index.
                        // Set to `{ lap: lap, index: index + 1 }`.
                        head + 1
                    } else {
                        // One lap forward, index wraps around to zero.
                        // Set to `{ lap: lap.wrapping_add(1), index: 0 }`.
                        lap.wrapping_add(self.one_lap)
                    };

                    // Try moving the head.
                    match self.head.compare_exchange_weak(
                        head,
                        new,
                        Ordering::SeqCst,
                        Ordering::Relaxed,
                    ) {
                        Ok(_) => {
                            // Read the value from the slot and update the stamp.
                            let msg = unsafe { slot.value.get().read() };
                            slot.stamp
                                .store(head.wrapping_add(self.one_lap), Ordering::Release);
                            return Ok(msg);
                        }
                        Err(h) => {
                            head = h;
                            backoff.spin();
                        }
                    }
                } else if stamp == head {
                    atomic::fence(Ordering::SeqCst);
                    let tail = self.tail.load(Ordering::Relaxed);

                    // If the tail equals the head, that means the channel is empty.
                    if tail == head {
                        return Err(PopError);
                    }

                    backoff.spin();
                    head = self.head.load(Ordering::Relaxed);
                } else {
                    // Snooze because we need to wait for the stamp to get updated.
                    backoff.snooze();
                    head = self.head.load(Ordering::Relaxed);
                }
            }
        }

        /// Returns the capacity of the queue.
        ///
        /// # Examples
        ///
        /// ```
        /// use crossbeam_queue::{ArrayQueue, PopError};
        ///
        /// let q = ArrayQueue::<i32>::new(100);
        ///
        /// assert_eq!(q.capacity(), 100);
        /// ```
        pub fn capacity(&self) -> usize {
            self.cap
        }

        /// Returns `true` if the queue is empty.
        ///
        /// # Examples
        ///
        /// ```
        /// use crossbeam_queue::{ArrayQueue, PopError};
        ///
        /// let q = ArrayQueue::new(100);
        ///
        /// assert!(q.is_empty());
        /// q.push(1).unwrap();
        /// assert!(!q.is_empty());
        /// ```
        pub fn is_empty(&self) -> bool {
            let head = self.head.load(Ordering::SeqCst);
            let tail = self.tail.load(Ordering::SeqCst);

            // Is the tail lagging one lap behind head?
            // Is the tail equal to the head?
            //
            // Note: If the head changes just before we load the tail, that means there was a moment
            // when the channel was not empty, so it is safe to just return `false`.
            tail == head
        }

        /// Returns `true` if the queue is full.
        ///
        /// # Examples
        ///
        /// ```
        /// use crossbeam_queue::{ArrayQueue, PopError};
        ///
        /// let q = ArrayQueue::new(1);
        ///
        /// assert!(!q.is_full());
        /// q.push(1).unwrap();
        /// assert!(q.is_full());
        /// ```
        pub fn is_full(&self) -> bool {
            let tail = self.tail.load(Ordering::SeqCst);
            let head = self.head.load(Ordering::SeqCst);

            // Is the head lagging one lap behind tail?
            //
            // Note: If the tail changes just before we load the head, that means there was a moment
            // when the queue was not full, so it is safe to just return `false`.
            head.wrapping_add(self.one_lap) == tail
        }

        /// Returns the number of elements in the queue.
        ///
        /// # Examples
        ///
        /// ```
        /// use crossbeam_queue::{ArrayQueue, PopError};
        ///
        /// let q = ArrayQueue::new(100);
        /// assert_eq!(q.len(), 0);
        ///
        /// q.push(10).unwrap();
        /// assert_eq!(q.len(), 1);
        ///
        /// q.push(20).unwrap();
        /// assert_eq!(q.len(), 2);
        /// ```
        pub fn len(&self) -> usize {
            loop {
                // Load the tail, then load the head.
                let tail = self.tail.load(Ordering::SeqCst);
                let head = self.head.load(Ordering::SeqCst);

                // If the tail didn't change, we've got consistent values to work with.
                if self.tail.load(Ordering::SeqCst) == tail {
                    let hix = head & (self.one_lap - 1);
                    let tix = tail & (self.one_lap - 1);

                    return if hix < tix {
                        tix - hix
                    } else if hix > tix {
                        self.cap - hix + tix
                    } else if tail == head {
                        0
                    } else {
                        self.cap
                    };
                }
            }
        }
    }

    // TODO - re-evaluate the Drop implementation in the absence of a Vec
    impl<T> Drop for ArrayQueue<T> {
        fn drop(&mut self) {
            // Get the index of the head.
            let hix = self.head.load(Ordering::Relaxed) & (self.one_lap - 1);

            // Loop over all slots that hold a message and drop them.
            for i in 0..self.len() {
                // Compute the index of the next slot holding a message.
                let index = if hix + i < self.cap {
                    hix + i
                } else {
                    hix + i - self.cap
                };

                unsafe {
                    self.buffer.add(index).drop_in_place();
                }
            }

            // TODO - re-evaluate the Drop implementation in the absence of a Vec
            // Finally, deallocate the buffer, but don't run any destructors.
            //unsafe {
            //    Vec::from_raw_parts(self.buffer, 0, self.cap);
            //}
        }
    }

    /// Error which occurs when popping from an empty queue.
    #[derive(Clone, Copy, Eq, PartialEq)]
    pub struct PopError;
    /// Error which occurs when pushing into a full queue.
    #[derive(Clone, Copy, Eq, PartialEq)]
    pub struct PushError<T>(pub T);


    const SPIN_LIMIT: u32 = 6;
    const YIELD_LIMIT: u32 = 10;
    pub struct Backoff {
        step: Cell<u32>,
    }

    impl Backoff {
        /// Creates a new `Backoff`.
        ///
        /// # Examples
        ///
        /// ```
        /// use crossbeam_utils::Backoff;
        ///
        /// let backoff = Backoff::new();
        /// ```
        #[inline]
        pub fn new() -> Self {
            Backoff {
                step: Cell::new(0),
            }
        }

        /// Resets the `Backoff`.
        ///
        /// # Examples
        ///
        /// ```
        /// use crossbeam_utils::Backoff;
        ///
        /// let backoff = Backoff::new();
        /// backoff.reset();
        /// ```
        #[inline]
        pub fn reset(&self) {
            self.step.set(0);
        }

        /// Backs off in a lock-free loop.
        ///
        /// This method should be used when we need to retry an operation because another thread made
        /// progress.
        ///
        /// The processor may yield using the *YIELD* or *PAUSE* instruction.
        ///
        /// # Examples
        ///
        /// Backing off in a lock-free loop:
        ///
        /// ```
        /// use crossbeam_utils::Backoff;
        /// use std::sync::atomic::AtomicUsize;
        /// use std::sync::atomic::Ordering::SeqCst;
        ///
        /// fn fetch_mul(a: &AtomicUsize, b: usize) -> usize {
        ///     let backoff = Backoff::new();
        ///     loop {
        ///         let val = a.load(SeqCst);
        ///         if a.compare_and_swap(val, val.wrapping_mul(b), SeqCst) == val {
        ///             return val;
        ///         }
        ///         backoff.spin();
        ///     }
        /// }
        ///
        /// let a = AtomicUsize::new(7);
        /// assert_eq!(fetch_mul(&a, 8), 7);
        /// assert_eq!(a.load(SeqCst), 56);
        /// ```
        #[inline]
        pub fn spin(&self) {
            for _ in 0..1 << self.step.get().min(SPIN_LIMIT) {
                atomic::spin_loop_hint();
            }

            if self.step.get() <= SPIN_LIMIT {
                self.step.set(self.step.get() + 1);
            }
        }

        /// Backs off in a blocking loop.
        ///
        /// This method should be used when we need to wait for another thread to make progress.
        ///
        /// The processor may yield using the *YIELD* or *PAUSE* instruction and the current thread
        /// may yield by giving up a timeslice to the OS scheduler.
        ///
        /// In `#[no_std]` environments, this method is equivalent to [`spin`].
        ///
        /// If possible, use [`is_completed`] to check when it is advised to stop using backoff and
        /// block the current thread using a different synchronization mechanism instead.
        ///
        /// [`spin`]: struct.Backoff.html#method.spin
        /// [`is_completed`]: struct.Backoff.html#method.is_completed
        ///
        /// # Examples
        ///
        /// Waiting for an [`AtomicBool`] to become `true`:
        ///
        /// ```
        /// use crossbeam_utils::Backoff;
        /// use std::sync::Arc;
        /// use std::sync::atomic::AtomicBool;
        /// use std::sync::atomic::Ordering::SeqCst;
        /// use std::thread;
        /// use std::time::Duration;
        ///
        /// fn spin_wait(ready: &AtomicBool) {
        ///     let backoff = Backoff::new();
        ///     while !ready.load(SeqCst) {
        ///         backoff.snooze();
        ///     }
        /// }
        ///
        /// let ready = Arc::new(AtomicBool::new(false));
        /// let ready2 = ready.clone();
        ///
        /// thread::spawn(move || {
        ///     thread::sleep(Duration::from_millis(100));
        ///     ready2.store(true, SeqCst);
        /// });
        ///
        /// assert_eq!(ready.load(SeqCst), false);
        /// spin_wait(&ready);
        /// assert_eq!(ready.load(SeqCst), true);
        /// ```
        ///
        /// [`AtomicBool`]: https://doc.rust-lang.org/std/sync/atomic/struct.AtomicBool.html
        #[inline]
        pub fn snooze(&self) {
            if self.step.get() <= SPIN_LIMIT {
                for _ in 0..1 << self.step.get() {
                    atomic::spin_loop_hint();
                }
            } else {
                #[cfg(not(feature = "std"))]
                    for _ in 0..1 << self.step.get() {
                    atomic::spin_loop_hint();
                }

                #[cfg(feature = "std")]
                    ::std::thread::yield_now();
            }

            if self.step.get() <= YIELD_LIMIT {
                self.step.set(self.step.get() + 1);
            }
        }

        /// Returns `true` if exponential backoff has completed and blocking the thread is advised.
        ///
        /// # Examples
        ///
        /// Waiting for an [`AtomicBool`] to become `true` and parking the thread after a long wait:
        ///
        /// ```
        /// use crossbeam_utils::Backoff;
        /// use std::sync::Arc;
        /// use std::sync::atomic::AtomicBool;
        /// use std::sync::atomic::Ordering::SeqCst;
        /// use std::thread;
        /// use std::time::Duration;
        ///
        /// fn blocking_wait(ready: &AtomicBool) {
        ///     let backoff = Backoff::new();
        ///     while !ready.load(SeqCst) {
        ///         if backoff.is_completed() {
        ///             thread::park();
        ///         } else {
        ///             backoff.snooze();
        ///         }
        ///     }
        /// }
        ///
        /// let ready = Arc::new(AtomicBool::new(false));
        /// let ready2 = ready.clone();
        /// let waiter = thread::current();
        ///
        /// thread::spawn(move || {
        ///     thread::sleep(Duration::from_millis(100));
        ///     ready2.store(true, SeqCst);
        ///     waiter.unpark();
        /// });
        ///
        /// assert_eq!(ready.load(SeqCst), false);
        /// blocking_wait(&ready);
        /// assert_eq!(ready.load(SeqCst), true);
        /// ```
        ///
        /// [`AtomicBool`]: https://doc.rust-lang.org/std/sync/atomic/struct.AtomicBool.html
        #[inline]
        pub fn is_completed(&self) -> bool {
            self.step.get() > YIELD_LIMIT
        }

        #[inline]
        #[doc(hidden)]
        #[deprecated(note = "use `is_completed` instead")]
        pub fn is_complete(&self) -> bool {
            self.is_completed()
        }
    }

    impl fmt::Debug for Backoff {
        fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
            f.debug_struct("Backoff")
                .field("step", &self.step)
                .field("is_completed", &self.is_completed())
                .finish()
        }
    }

    impl Default for Backoff {
        fn default() -> Backoff {
            Backoff::new()
        }
    }
    // --------------

    //pub fn lamport_queue_pair<
    //    ScratchFreeSlots: Unsigned,
    //    CallerFreeSlots: Unsigned,
    //    ResponderFreeSlots: Unsigned,
    //    CallerPageDirFreeSlots: Unsigned,
    //    CallerPageTableFreeSlots: Unsigned,
    //    CallerFilledPageTableCount: Unsigned,
    //    ResponderPageDirFreeSlots: Unsigned,
    //    ResponderPageTableFreeSlots: Unsigned,
    //    ResponderFilledPageTableCount: Unsigned,
    //    T: Send + Sync,
    //>(
    //    local_cnode: LocalCap<LocalCNode<ScratchFreeSlots>>,
    //    shared_page_ut: LocalCap<Untyped<U12>>,
    //    vspace_a: VSpace<
    //        CallerPageDirFreeSlots,
    //        CallerPageTableFreeSlots,
    //        CallerFilledPageTableCount,
    //        role::Child,
    //    >,
    //    vspace_b: VSpace<
    //        ResponderPageDirFreeSlots,
    //        ResponderPageTableFreeSlots,
    //        ResponderFilledPageTableCount,
    //        role::Child,
    //    >,
    //    // TODO - may be able to remove these if we ultimately need to add zero caps
    //    child_cnode_a: LocalCap<ChildCNode<CallerFreeSlots>>,
    //    child_cnode_b: LocalCap<ChildCNode<ResponderFreeSlots>>,
    //) -> Result<
    //    (
    //        LocalCap<ChildCNode<Diff<CallerFreeSlots, U2>>>,
    //        LocalCap<ChildCNode<Diff<ResponderFreeSlots, U2>>>,
    //        // TODO - more closely associate these with the process-children
    //        // that they *must* interoperate with
    //        LamportHandle<T, role::Child>,
    //        LamportHandle<T, role::Child>,
    //        VSpace<
    //            CallerPageDirFreeSlots,
    //            Sub1<CallerPageTableFreeSlots>,
    //            CallerFilledPageTableCount,
    //            role::Child,
    //        >,
    //        VSpace<
    //            ResponderPageDirFreeSlots,
    //            Sub1<ResponderPageTableFreeSlots>,
    //            ResponderFilledPageTableCount,
    //            role::Child,
    //        >,
    //        LocalCap<LocalCNode<Diff<ScratchFreeSlots, U5>>>,
    //    ),
    //    IPCError,
    //>
    //    where
    //        ScratchFreeSlots: Sub<U5>,
    //        Diff<ScratchFreeSlots, U5>: Unsigned,

    //        CallerPageTableFreeSlots: Sub<B1>,
    //        Sub1<CallerPageTableFreeSlots>: Unsigned,

    //        ResponderPageTableFreeSlots: Sub<B1>,
    //        Sub1<ResponderPageTableFreeSlots>: Unsigned,

    //        CallerFreeSlots: Sub<U2>,
    //        Diff<CallerFreeSlots, U2>: Unsigned,

    //        ResponderFreeSlots: Sub<U2>,
    //        Diff<ResponderFreeSlots, U2>: Unsigned,

    //        CallerFilledPageTableCount: ArrayLength<LocalCap<MappedPageTable<U0, role::Child>>>,
    //        ResponderFilledPageTableCount: ArrayLength<LocalCap<MappedPageTable<U0, role::Child>>>,
    //{
    //    let element_size = core::mem::size_of::<T>();
    //    // TODO - Move this to compile-time somehow
    //    if element_size > PageBytes::USIZE {
    //        return Err(IPCError::RequestSizeTooBig);
    //    }
    //    let queue_size = core::mem::size_of::<LamportQueue<T>>();
    //    if queue_size > PageBytes::USIZE {
    //        return Err(IPCError::RequestSizeTooBig);
    //    }
    //    // TODO - temporarily map the page locally so we can initialize the queue

    //    let (local_cnode, remainder_local_cnode) = local_cnode.reserve_region::<U5>();
    //    let (child_cnode_a, remainder_child_cnode_a) =
    //        child_cnode_a.reserve_region::<U2>();
    //    let (child_cnode_b, remainder_child_cnode_b) =
    //        child_cnode_b.reserve_region::<U2>();

    //    let (shared_page, local_cnode) = shared_page_ut.retype_local::<_, UnmappedPage>(local_cnode)?;

    //    let (shared_page_a, local_cnode) =
    //        shared_page.copy_inside_cnode(local_cnode, CapRights::RW)?;
    //    let (shared_page_mapped_a, vspace_a) = vspace_a.map_page(shared_page_a)?;

    //    let (shared_page_b, local_cnode) =
    //        shared_page.copy_inside_cnode(local_cnode, CapRights::RW)?;
    //    let (shared_page_mapped_b, vspace_b) =
    //        vspace_b.map_page(shared_page_b)?;

    //    let queue_handle_a = LamportHandle {
    //        shared_page_address: unsafe {shared_page_mapped_a.cap_data.vaddr as *mut LamportQueue<T>},
    //        _t: PhantomData,
    //        _role: PhantomData,
    //    };
    //    let queue_handle_b = LamportHandle {
    //        shared_page_address: unsafe {shared_page_mapped_b.cap_data.vaddr as *mut LamportQueue<T>},
    //        _t: PhantomData,
    //        _role: PhantomData,
    //    };
    //    Ok((
    //        remainder_child_cnode_a,
    //        remainder_child_cnode_b,
    //        queue_handle_a,
    //        queue_handle_b,
    //        vspace_a,
    //        vspace_b,
    //        remainder_local_cnode,
    //    ))
    //}
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
            Badge::from(0x01),
        )?;
        let (caller_response_ready, _child_cnode_caller) = local_response_ready.mint(
            &local_cnode,
            child_cnode_caller,
            CapRights::RWG,
            Badge::from(0x10),
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
            Badge::from(0x100),
        )?;
        let (responder_response_ready, _child_cnode_responder) = local_response_ready.mint(
            &local_cnode,
            child_cnode_responder,
            CapRights::RWG,
            Badge::from(0x1000),
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
        pub fn reply_recv<F>(self, f: F) -> Result<Rsp, IPCError>
        where
            F: Fn(&Req) -> (Rsp),
        {
            self.reply_recv_with_state((), move |req, state| (f(req), state))
        }

        pub fn reply_recv_with_state<F, State>(
            self,
            initial_state: State,
            f: F,
        ) -> Result<Rsp, IPCError>
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
