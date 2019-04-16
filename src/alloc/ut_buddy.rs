/// UTBuddy is a type-safe static buddy allocator for Untyped capabilites.
use arrayvec::ArrayVec;
use core::marker::PhantomData;
use core::ops::Add;
use core::ops::Sub;
use sel4_sys::*;
use typenum::*;

use crate::arch::kernel;
use crate::config;
use crate::userland::{Cap, LocalCNodeSlots, LocalCap, SeL4Error, Untyped};

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

// Buddy alloc
pub struct UTBuddy<PoolSizes: UList> {
    _pool_sizes: PhantomData<PoolSizes>,
    pool: [ArrayVec<[usize; config::UTPoolSlotsPerSize::USIZE]>; kernel::MaxUntypedSize::USIZE],
}

#[allow(dead_code)]
fn print_pool(
    pool: &[ArrayVec<[usize; config::UTPoolSlotsPerSize::USIZE]>; kernel::MaxUntypedSize::USIZE],
) {
    debug_print!("Pool[ ");
    for av in pool {
        debug_print!("{} ", av.len());
    }
    debug_println!("]");
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
    let mut pool: [ArrayVec<[usize; config::UTPoolSlotsPerSize::USIZE]>;
        kernel::MaxUntypedSize::USIZE] = Default::default();

    pool[BitSize::USIZE - kernel::MinUntypedSize::USIZE].push(ut.cptr);

    UTBuddy {
        _pool_sizes: PhantomData,
        pool: pool,
    }
}

impl<PoolSizes: UList> UTBuddy<PoolSizes> {
    pub fn alloc<BitSize: Unsigned, SlotCount: Unsigned>(
        self,
        slots: LocalCNodeSlots<SlotCount>,
    ) -> Result<
        (
            LocalCap<Untyped<BitSize>>,
            UTBuddy<TakeUntyped_ResultPoolSizes<PoolSizes, Diff<BitSize, kernel::MinUntypedSize>>>,
        ),
        SeL4Error,
    >
    where
        BitSize: Sub<kernel::MinUntypedSize>,
        PoolSizes: _TakeUntyped<Diff<BitSize, kernel::MinUntypedSize>, NumSplits = SlotCount>,
        TakeUntyped_ResultPoolSizes<PoolSizes, Diff<BitSize, kernel::MinUntypedSize>>: UList,
    {
        // The index in the pool array where Untypeds of the requested size are stored.
        let index = BitSize::USIZE - kernel::MinUntypedSize::USIZE;
        // let cptr_bitsize = index + kernel::MinUntypedSize::USIZE;
        // debug_println!("*** Requested a UT{}", cptr_bitsize);

        let mut pool = self.pool;
        // debug_print!("*** Initial pool: ");
        // print_pool(&pool);

        // If there's no cptr of the requested size, make one by splitting the larger ones.
        if pool[index].len() == 0 {
            let split_start_index = index + SlotCount::USIZE;
            for (i, slot) in (index..=split_start_index).rev().zip(slots.iter()) {
                let cptr = pool[i].pop().unwrap();
                let cptr_bitsize = i + kernel::MinUntypedSize::USIZE;

                // debug_println!("*** Splitting a UT{}", cptr_bitsize);
                let (slot_cptr, slot_offset, _) = slot.elim();

                let err = unsafe {
                    seL4_Untyped_Retype(
                        cptr,                                   // _service
                        api_object_seL4_UntypedObject as usize, // type
                        cptr_bitsize - 1,                       // size_bits
                        slot_cptr,                              // root
                        0,                                      // index
                        0,                                      // depth
                        slot_offset,                            // offset
                        1,                                      // num_objects
                    )
                };
                if err != 0 {
                    return Err(SeL4Error::UntypedRetype(err));
                }

                pool[i - 1].push(cptr);
                pool[i - 1].push(slot_offset);
            }
        }

        let cptr = pool[index].pop().unwrap();
        // debug_println!("*** Returning a UT{}", cptr_bitsize);
        // debug_print!("*** New pool:     ");
        // print_pool(&pool);

        Ok((
            Cap::wrap_cptr(cptr),
            UTBuddy {
                _pool_sizes: PhantomData,
                pool: pool,
            },
        ))
    }
}

// trait UTBuddyAlloc<SlotCount: Unsigned, PoolSizes: UList, BitSize: Unsigned>
// where
//     BitSize: Sub<kernel::MinUntypedSize>,
//     PoolSizes: _TakeUntyped<Diff<BitSize, kernel::MinUntypedSize>, NumSplits = SlotCount>,
//     TakeUntyped_ResultPoolSizes<PoolSizes, Diff<BitSize, kernel::MinUntypedSize>>: UList,
// {
//     fn alloc(
//         self,
//         slots: LocalCNodeSlots<SlotCount>,
//     ) -> (
//         LocalCap<Untyped<BitSize>>,
//         UTBuddy<TakeUntyped_ResultPoolSizes<PoolSizes, Diff<BitSize, kernel::MinUntypedSize>>>,
//     );
// }

// impl<PoolSizes: UList, BitSize: Unsigned> UTBuddyAlloc<U0, PoolSizes, BitSize>
//     for UTBuddy<PoolSizes>
// where
//     BitSize: Sub<kernel::MinUntypedSize>,
//     PoolSizes: _TakeUntyped<Diff<BitSize, kernel::MinUntypedSize>, NumSplits = U0>,
//     TakeUntyped_ResultPoolSizes<PoolSizes, Diff<BitSize, kernel::MinUntypedSize>>: UList,
// {
//     fn alloc(
//         self,
//         slots: LocalCNodeSlots<U0>,
//     ) -> (
//         LocalCap<Untyped<BitSize>>,
//         UTBuddy<TakeUntyped_ResultPoolSizes<PoolSizes, Diff<BitSize, kernel::MinUntypedSize>>>,
//     ) {
//         let index = BitSize::USIZE - kernel::MinUntypedSize::USIZE;
//         let mut pool = self.pool;
//         match pool[index].pop() {
//             Some(cptr) => (
//                 Cap::wrap_cptr(cptr),
//                 UTBuddy {
//                     _pool_sizes: PhantomData,
//                     pool: pool,
//                 },
//             ),
//             None => {
//                 // This should be entirely unreachable
//                 panic!()
//             }
//         }
//     }
// }

// impl<SCHead: Bit, SCTail: Unsigned, PoolSizes: UList, BitSize: Unsigned>
//     UTBuddyAlloc<UInt<SCTail, SCHead>, PoolSizes, BitSize> for UTBuddy<PoolSizes>
// where
//     BitSize: Sub<kernel::MinUntypedSize>,
//     PoolSizes:
//         _TakeUntyped<Diff<BitSize, kernel::MinUntypedSize>, NumSplits = UInt<SCTail, SCHead>>,
//     TakeUntyped_ResultPoolSizes<PoolSizes, Diff<BitSize, kernel::MinUntypedSize>>: UList,
//     UInt<SCTail, SCHead>: Sub<U1>,
//     Diff<UInt<SCTail, SCHead>, U1>: Unsigned,
//     BitSize: Add<U1>,
//     Sum<BitSize, U1>: Unsigned,
//     Sum<BitSize, U1>: Sub<U1>,
//     Diff<Sum<BitSize, U1>, U1>: Unsigned,
// Sum<BitSize, U1>: Sub<U4>,
// Diff<Sum<BitSize, U1>, U4>: Unsigned,
// {
//     fn alloc(
//         self,
//         slots: LocalCNodeSlots<UInt<SCTail, SCHead>>,
//     ) -> (
//         LocalCap<Untyped<BitSize>>,
//         UTBuddy<TakeUntyped_ResultPoolSizes<PoolSizes, Diff<BitSize, kernel::MinUntypedSize>>>,
//     ) {
//         let index = BitSize::USIZE - kernel::MinUntypedSize::USIZE;
//         assert!(self.pool[index].len() == 0);

//         let (slot, slots): (LocalCNodeSlot, _) = slots.alloc();
//         let (ut, buddy): (LocalCap<Untyped<op!{BitSize + U1}>>, _) = self.alloc(slots);
//         let (ut_a, ut_b) = ut.split(slot).unwrap();
//         buddy.pool[index][0] = ut_a.cptr;
//         (ut_b, buddy)
//     }
// }
