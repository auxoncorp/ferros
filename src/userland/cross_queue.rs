//! A lock free queue implementation adapted from Crossbeam's `ArrayQueue`:
//! https://github.com/crossbeam-rs/crossbeam
//! TODO: Mention license? crossbeam is dual apache / MIT.

use core::cell::{Cell, UnsafeCell};
use core::fmt;
use core::marker::PhantomData;
use core::ops::{Deref, DerefMut};
use core::sync::atomic::{self, AtomicUsize, Ordering};

use generic_array::sequence::GenericSequence;
use generic_array::{ArrayLength, GenericArray};

use typenum::consts::U0;
use typenum::{IsGreater, Unsigned};

/// A slot in a queue.
pub struct Slot<T> {
    /// The current stamp.
    ///
    /// If the stamp equals the tail, this node will be next written
    /// to. If it equals the head, this node will be next read from.
    stamp: AtomicUsize,

    /// The value in this slot.
    value: UnsafeCell<T>,
}

unsafe impl<T: Send, Size: Unsigned> Send for ArrayQueue<T, Size>
where
    Size: IsGreater<U0>,
    Size: ArrayLength<Slot<T>>,
{
}

unsafe impl<T: Send, Size: Unsigned> Sync for ArrayQueue<T, Size>
where
    Size: IsGreater<U0>,
    Size: ArrayLength<Slot<T>>,
{
}

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
    /// use ferros::userland::cross_queue::CachePadded;
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
    /// use ferros::userland::cross_queue::CachePadded;
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

pub struct ArrayQueue<T, Size: Unsigned>
where
    Size: IsGreater<U0>,
    Size: ArrayLength<Slot<T>>,
{
    /// The head of the queue.
    ///
    /// This value is a "stamp" consisting of an index into the buffer
    /// and a lap, but packed into a single `usize`. The lower bits
    /// represent the index, while the upper bits represent the lap.
    ///
    /// Elements are popped from the head of the queue.
    head: CachePadded<AtomicUsize>,

    /// The tail of the queue.
    ///
    /// This value is a "stamp" consisting of an index into the buffer
    /// and a lap, but packed into a single `usize`. The lower bits
    /// represent the index, while the upper bits represent the lap.
    ///
    /// Elements are pushed into the tail of the queue.
    tail: CachePadded<AtomicUsize>,

    /// The buffer holding slots.
    buffer: UnsafeCell<GenericArray<Slot<T>, Size>>,

    /// The queue capacity.
    cap: usize,

    /// A stamp with the value of `{ lap: 1, index: 0 }`.
    one_lap: usize,

    /// Indicates that dropping an `ArrayQueue<T>` may drop elements
    /// of type `T`.
    _marker: PhantomData<T>,

    _size: PhantomData<Size>,
}

impl<T, Size: Unsigned> ArrayQueue<T, Size>
where
    Size: IsGreater<U0>,
    Size: ArrayLength<Slot<T>>,
{
    /// Creates a new bounded queue with the capacity `Size`.
    ///
    /// ```
    /// use ferros::usersland::cross_queue::ArrayQueue;
    ///
    /// let q = ArrayQueue::<i32, U100>::new();
    /// ```
    pub fn new() -> Self {
        // Head is initialized to `{ lap: 0, index: 0 }`.
        // Tail is initialized to `{ lap: 0, index: 0 }`.
        let head = 0;
        let tail = 0;

        // One lap is the smallest power of two greater than `cap`.
        let one_lap = (Size::USIZE + 1).next_power_of_two();

        ArrayQueue {
            buffer: UnsafeCell::new(GenericArray::generate(move |i| Slot {
                stamp: AtomicUsize::new(i),
                value: unsafe { core::mem::zeroed() },
            })),
            one_lap: one_lap,
            cap: Size::USIZE,
            head: CachePadded::new(AtomicUsize::new(head)),
            tail: CachePadded::new(AtomicUsize::new(tail)),
            _marker: PhantomData,
            _size: PhantomData,
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
            let slot = &mut unsafe { &mut *self.buffer.get() }[index];
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
            let slot = &mut unsafe { &mut *self.buffer.get() }[index];
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

impl<T, Size: Unsigned> Drop for ArrayQueue<T, Size>
where
    Size: IsGreater<U0>,
    Size: ArrayLength<Slot<T>>,
{
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
                (&mut (*self.buffer.get())[index] as *mut Slot<T>).drop_in_place();
            }
        }

        // TODO - re-evaluate the Drop implementation in the absence of a Vec
        // The original implementation in crossbeam-queue had the intent of
        // deallocating the buffer without running any destructors for the members.
        //
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
        Backoff { step: Cell::new(0) }
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
