/// UTBuddy is a type-safe static buddy allocator for Untyped capabilites.
use arrayvec::ArrayVec;
use core::marker::PhantomData;
use core::ops::{Add, Mul, Sub};
use selfe_sys::*;
use typenum::*;

use crate::arch::{MaxUntypedSize, MinUntypedSize};
use crate::cap::{
    memory_kind, role, CNodeRole, Cap, LocalCNode, LocalCNodeSlot, LocalCNodeSlots, LocalCap,
    PhantomCap, Untyped, WCNodeSlots, WCNodeSlotsData, WUntyped,
};
use crate::error::{ErrorExt, SeL4Error};

type UTPoolSlotsPerSize = U4;

/// A type-level linked list of typenum::Unsigned.
pub trait UList {
    type Length: Unsigned;
}

/// The empty list
pub struct ULNull {}

/// A cell in the linked list
pub struct ULCons<Head: Unsigned, Tail: UList> {
    _head: PhantomData<Head>,
    _tail: PhantomData<Tail>,
}

impl UList for ULNull {
    type Length = U0;
}

impl<Head: Unsigned, Tail: UList> UList for ULCons<Head, Tail>
where
    U1: Add<Tail::Length>,
    Sum<U1, Tail::Length>: Unsigned,
{
    type Length = Sum<U1, Tail::Length>;
}

/// Type-level function to initialize a ulist with a single index set to 1
pub trait _OneHotUList: Unsigned {
    type Output;
}

type OneHotUList<Index> = <Index as _OneHotUList>::Output;

impl _OneHotUList for U0 {
    type Output = ULCons<U1, ULNull>;
}

impl<IHead: Bit, ITail: Unsigned> _OneHotUList for UInt<ITail, IHead>
where
    UInt<ITail, IHead>: Sub<U1>,
    Diff<UInt<ITail, IHead>, U1>: _OneHotUList,
    OneHotUList<Diff<UInt<ITail, IHead>, U1>>: UList,
{
    type Output = ULCons<U0, OneHotUList<Diff<UInt<ITail, IHead>, U1>>>;
}

/// Type-level function to track the result of an allocation
pub trait _TakeUntyped<Index> {
    type ResultPoolSizes;
    type NumSplits;
}

// Index is non-zero, and there are pools left: recur with Index-1, and the
// remaining pools
impl<IndexU: Unsigned, IndexB: Bit, Head: Unsigned, Tail: UList> _TakeUntyped<UInt<IndexU, IndexB>>
    for ULCons<Head, Tail>
where
    UInt<IndexU, IndexB>: Sub<U1>,
    Diff<UInt<IndexU, IndexB>, U1>: Unsigned,

    Tail: _TakeUntyped<Diff<UInt<IndexU, IndexB>, U1>>,
    TakeUntyped_ResultPoolSizes<Tail, Diff<UInt<IndexU, IndexB>, U1>>: UList,
{
    type ResultPoolSizes =
        ULCons<Head, TakeUntyped_ResultPoolSizes<Tail, Diff<UInt<IndexU, IndexB>, U1>>>;
    type NumSplits = TakeUntyped_NumSplits<Tail, Diff<UInt<IndexU, IndexB>, U1>>;
}

// Index is 0, and the head pool has resources: remove one from it, with no splits.
impl<HeadU: Unsigned, HeadB: Bit, Tail: UList> _TakeUntyped<U0> for ULCons<UInt<HeadU, HeadB>, Tail>
where
    UInt<HeadU, HeadB>: Sub<U1>,
    Diff<UInt<HeadU, HeadB>, U1>: Unsigned,
{
    type ResultPoolSizes = ULCons<Diff<UInt<HeadU, HeadB>, U1>, Tail>;
    type NumSplits = U0;
}

// index is zero, and the head pool is empty. Take one from the next pool (which
// we will split, and return one of), and put one (the remainder) in the head
// pool.
impl<Tail: UList> _TakeUntyped<U0> for ULCons<U0, Tail>
where
    Tail: _TakeUntyped<U0>,
    U1: Add<TakeUntyped_NumSplits<Tail, U0>>,
    Sum<U1, TakeUntyped_NumSplits<Tail, U0>>: Unsigned,
    TakeUntyped_ResultPoolSizes<Tail, U0>: UList,
{
    type ResultPoolSizes = ULCons<U1, TakeUntyped_ResultPoolSizes<Tail, U0>>;
    type NumSplits = Sum<U1, TakeUntyped_NumSplits<Tail, U0>>;
}

#[allow(non_camel_case_types)]
type TakeUntyped_ResultPoolSizes<PoolSizes, Index> =
    <PoolSizes as _TakeUntyped<Index>>::ResultPoolSizes;

#[allow(non_camel_case_types)]
type TakeUntyped_NumSplits<PoolSizes, Index> = <PoolSizes as _TakeUntyped<Index>>::NumSplits;

/// Buddy alloc
///
/// Presently restricted to provide memory_kind::General untyped
pub struct UTBuddy<PoolSizes: UList> {
    _pool_sizes: PhantomData<PoolSizes>,
    pool: [ArrayVec<[usize; UTPoolSlotsPerSize::USIZE]>; MaxUntypedSize::USIZE],
}

/// Make a new UTBuddy by wrapping an untyped.
pub fn ut_buddy<BitSize: Unsigned>(
    ut: LocalCap<Untyped<BitSize>>,
) -> UTBuddy<OneHotUList<Diff<BitSize, U4>>>
where
    BitSize: Sub<U4>,
    Diff<BitSize, U4>: _OneHotUList,
    OneHotUList<Diff<BitSize, U4>>: UList,
{
    let mut pool = make_pool();
    pool[BitSize::USIZE - MinUntypedSize::USIZE].push(ut.cptr);

    UTBuddy {
        _pool_sizes: PhantomData,
        pool,
    }
}

impl<PoolSizes: UList> UTBuddy<PoolSizes> {
    pub fn alloc<BitSize: Unsigned, NumSplits: Unsigned>(
        mut self,
        slots: LocalCNodeSlots<Prod<NumSplits, U2>>,
    ) -> Result<
        (
            LocalCap<Untyped<BitSize, memory_kind::General>>,
            UTBuddy<TakeUntyped_ResultPoolSizes<PoolSizes, Diff<BitSize, MinUntypedSize>>>,
        ),
        SeL4Error,
    >
    where
        BitSize: Sub<MinUntypedSize>,
        NumSplits: Mul<U2>,
        Prod<NumSplits, U2>: Unsigned,
        PoolSizes: _TakeUntyped<Diff<BitSize, MinUntypedSize>, NumSplits = NumSplits>,
        TakeUntyped_ResultPoolSizes<PoolSizes, Diff<BitSize, MinUntypedSize>>: UList,
    {
        let weak_ut = alloc(
            &mut self.pool,
            slots.iter(),
            BitSize::USIZE,
            NumSplits::USIZE,
        )?;
        Ok((
            Cap::wrap_cptr(weak_ut.cptr),
            UTBuddy {
                pool: self.pool,
                _pool_sizes: PhantomData,
            },
        ))
    }
}

/// Make a weak ut buddy around a weak untyped.
pub fn weak_ut_buddy<Role: CNodeRole>(
    ut: Cap<WUntyped<memory_kind::General>, Role>,
) -> WUTBuddy<Role> {
    let mut pool = make_pool();
    pool[ut.cap_data.size_bits - MinUntypedSize::USIZE].push(ut.cptr);
    WUTBuddy {
        pool,
        _role: PhantomData,
    }
}

/// The error returned when using the runtime-checked (weak)
/// realization of a ut buddy.
#[derive(Debug)]
pub enum UTBuddyError {
    /// The requested size exceeds max untyped size for this
    /// architecture.
    RequestedSizeExceedsMax(usize),
    /// There are not enough CNode slots to do the requisite
    /// splitting.
    NotEnoughSlots,
    /// The wrapped untyped lacks the sufficient size to do this
    /// allocation request.
    CannotAllocateRequestedSize(usize),
    /// We got an error from an seL4 syscall, namely the
    /// `seL4_Untyped_Retype` call.
    SeL4Error(SeL4Error),
}

impl From<SeL4Error> for UTBuddyError {
    fn from(e: SeL4Error) -> Self {
        UTBuddyError::SeL4Error(e)
    }
}

/// A weakened implementation of a UTBuddy allocator where the state
/// is checked at runtime rather than tracked in the types.
///
/// Presently restricted to provide memory_kind::General untyped
pub struct WUTBuddy<Role: CNodeRole = role::Local> {
    pool: [ArrayVec<[usize; UTPoolSlotsPerSize::USIZE]>; MaxUntypedSize::USIZE],
    _role: PhantomData<Role>,
}

impl WUTBuddy<role::Local> {
    /// Allocate a strong untyped from the pool.
    pub fn alloc_strong<Size: Unsigned>(
        &mut self,
        slots: &mut WCNodeSlots,
    ) -> Result<LocalCap<Untyped<Size>>, UTBuddyError> {
        let weak_ut = self.alloc(slots, Size::USIZE)?;
        Ok(Cap {
            cptr: weak_ut.cptr,
            cap_data: PhantomCap::phantom_instance(),
            _role: PhantomData,
        })
    }

    /// Allocate a weak untyped from the pool.
    pub fn alloc(
        &mut self,
        slots: &mut WCNodeSlots,
        size: usize,
    ) -> Result<LocalCap<WUntyped<memory_kind::General>>, UTBuddyError> {
        if size > MaxUntypedSize::USIZE {
            return Err(UTBuddyError::RequestedSizeExceedsMax(size));
        }

        let idx = size - MinUntypedSize::USIZE;

        // In the strong case, `NumSplits` can be inferred, however
        // with runtime data we must calculate this.
        let mut split_count = 0;
        let mut ut_big_enough = false;
        for i in idx..MaxUntypedSize::USIZE {
            if self.pool[i].len() == 0 {
                split_count += 1;
            } else {
                ut_big_enough = true;
                break;
            }
        }

        // If on our travels through the pool we never encountered a
        // pool slot which is /not/ empty, we cannot allocate the
        // requested size—our wrapped untyped is too small :(
        if !ut_big_enough {
            return Err(UTBuddyError::CannotAllocateRequestedSize(size));
        }

        let slot_count = split_count * 2;
        // We also need to confirm that we have enough slots.
        if slot_count > slots.cap_data.size {
            return Err(UTBuddyError::NotEnoughSlots);
        }

        let slots_for_alloc_to_consume = Cap {
            cptr: slots.cptr,
            cap_data: WCNodeSlotsData {
                offset: slots.cap_data.offset,
                size: slot_count,
                _role: PhantomData,
            },
            _role: PhantomData,
        };

        // account for the resources we've used on our borrowed set of
        // slots.
        slots.cap_data.offset = slots.cap_data.offset + slot_count;
        slots.cap_data.size = slots.cap_data.size - slot_count;

        let ut = alloc(
            &mut self.pool,
            slots_for_alloc_to_consume.into_strong_iter(),
            size,
            split_count,
        )?;
        Ok(ut)
    }

    fn total_occupied_slots(&self) -> usize {
        self.pool.iter().map(|sub_pool| sub_pool.len()).sum()
    }

    pub fn move_to_child(
        self,
        src_cnode: &LocalCap<LocalCNode>,
        slots: &mut LocalCap<WCNodeSlotsData<role::Child>>,
    ) -> Result<WUTBuddy<role::Child>, UTBuddyError> {
        if self.total_occupied_slots() > slots.cap_data.size {
            return Err(UTBuddyError::NotEnoughSlots);
        }
        // N.B. We could be reclaiming the emptied local slots for future use, but are currently not
        // purely for implementation-time-and-complexity reasons.
        let mut child_pool = make_pool();
        for ((i, local_bucket), child_bucket) in
            self.pool.iter().enumerate().zip(child_pool.iter_mut())
        {
            let size_bits = i + MinUntypedSize::USIZE;
            for (local_ut_cptr, dest_slot) in local_bucket
                .iter()
                .zip(slots.incrementally_consuming_iter())
            {
                let local_wut: Cap<WUntyped<memory_kind::General>, _> = Cap {
                    cptr: *local_ut_cptr,
                    cap_data: WUntyped {
                        size_bits,
                        // Note the strong assumption that WUTBuddy only represents memory_kind::General
                        kind: memory_kind::General,
                    },
                    _role: PhantomData,
                };
                let child_wut = local_wut.move_to_slot(src_cnode, dest_slot)?;
                child_bucket.push(child_wut.cptr);
            }
        }
        Ok(WUTBuddy {
            pool: child_pool,
            _role: PhantomData,
        })
    }
}

impl<Role: CNodeRole> WUTBuddy<Role> {
    pub(crate) fn empty() -> WUTBuddy<Role> {
        WUTBuddy {
            pool: make_pool(),
            _role: PhantomData,
        }
    }
}

fn alloc(
    pool: &mut [ArrayVec<[usize; UTPoolSlotsPerSize::USIZE]>; MaxUntypedSize::USIZE],
    slots_iter: impl Iterator<Item = LocalCNodeSlot>,
    size_bits: usize,
    split_count: usize,
) -> Result<LocalCap<WUntyped<memory_kind::General>>, SeL4Error> {
    // The index in the pool array where Untypeds of the requested
    // size are stored.
    let index = size_bits - MinUntypedSize::USIZE;

    // If there's no cptr of the requested size, make one by splitting
    // the larger ones.
    if pool[index].len() == 0 {
        let split_start_index = index + split_count;
        for (i, slot) in (index..=split_start_index).rev().zip(slots_iter.step_by(2)) {
            let cptr = pool[i].pop().unwrap();
            let cptr_bitsize = i + MinUntypedSize::USIZE;

            let (slot_cptr, slot_offset, _) = slot.elim();

            unsafe {
                seL4_Untyped_Retype(
                    cptr,                                   // _service
                    api_object_seL4_UntypedObject as usize, // type
                    cptr_bitsize - 1,                       // size_bits
                    slot_cptr,                              // root
                    0,                                      // index
                    0,                                      // depth
                    slot_offset,                            // offset
                    2,                                      // num_objects
                )
            }
            .as_result()
            .map_err(|e| SeL4Error::UntypedRetype(e))?;

            pool[i - 1].push(slot_offset);
            pool[i - 1].push(slot_offset + 1);
        }
    }

    let cptr = pool[index].pop().unwrap();

    Ok(Cap {
        cptr,
        cap_data: WUntyped {
            size_bits,
            kind: memory_kind::General,
        },
        _role: PhantomData,
    })
}

fn make_pool() -> [ArrayVec<[usize; UTPoolSlotsPerSize::USIZE]>; MaxUntypedSize::USIZE] {
    unsafe {
        let mut pool: [ArrayVec<[usize; UTPoolSlotsPerSize::USIZE]>; MaxUntypedSize::USIZE] =
            core::mem::uninitialized();
        for p in pool.iter_mut() {
            core::ptr::write(p, ArrayVec::default());
        }
        pool
    }
}
