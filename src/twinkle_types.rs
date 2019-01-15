// in the type, store a flag for each bit size to indicate whether we have an
// untyped of that size.

use crate::fancy::{role, CNode, CNodeRole, Cap, Untyped};
use crate::pow::{Pow, _Pow};
use core::marker::PhantomData;
use core::mem::transmute;
use core::ops::{Add, Sub};
use typenum::operator_aliases::{Add1, Diff, Shleft, Sub1, Sum};
use typenum::uint::{GetBit, GetBitOut, SetBit, SetBitOut};
use typenum::{
    Bit, UInt, UTerm, Unsigned, B0, B1, U0, U1, U10, U11, U12, U2, U256, U3, U4, U5, U6, U7, U8, U9,
};

const POOL_SIZE: usize = 32;

struct Allocator<Flags: Unsigned> {
    pool: [Option<usize>; POOL_SIZE],
    _flags: PhantomData<Flags>,
}

fn new_allocator<InitialBitSize: Unsigned>(
    ut: Cap<Untyped<InitialBitSize>, role::Local>,
) -> Allocator<Pow<InitialBitSize>>
where
    InitialBitSize: _Pow,
    Pow<InitialBitSize>: Unsigned,
{
    let mut pool = [None; POOL_SIZE];
    pool[InitialBitSize::to_usize()] = Some(ut.cptr);
    Allocator {
        pool,
        _flags: PhantomData,
    }
}

trait _TakeSlot<Index> {
    type OutputFlags;
    type NewUntypedCount;
}

type TakeSlot_Flags<Flags, Index> = <Flags as _TakeSlot<Index>>::OutputFlags;
type TakeSlot_NewUntypedCount<Flags, Index> = <Flags as _TakeSlot<Index>>::NewUntypedCount;

// index is non-zero, and there are flags left: recur with index-1, the other flags
impl<IU: Unsigned, IB: Bit, FU: Unsigned, FB: Bit> _TakeSlot<UInt<IU, IB>> for UInt<FU, FB>
where
    UInt<IU, IB>: Sub<B1>,
    Sub1<UInt<IU, IB>>: Unsigned,
    FU: _TakeSlot<Sub1<UInt<IU, IB>>>,
    TakeSlot_Flags<FU, Sub1<UInt<IU, IB>>>: Unsigned,
{
    type OutputFlags = UInt<TakeSlot_Flags<FU, Sub1<UInt<IU, IB>>>, FB>;
    type NewUntypedCount = <FU as _TakeSlot<Sub1<UInt<IU, IB>>>>::NewUntypedCount;
}

// index is zero, and the bottom flag is 1: consume. (set to zero)
impl<FU: Unsigned> _TakeSlot<U0> for UInt<FU, B1> {
    type OutputFlags = UInt<FU, B0>;
    type NewUntypedCount = U0;
}

// index is zero, and the bottom flag is 0: Take the next slot up, and set the
// bottom flag to 1 (since we're splitting the next slot).
impl<FU: Unsigned> _TakeSlot<U0> for UInt<FU, B0>
where
    FU: _TakeSlot<U0>,
    TakeSlot_Flags<FU, U0>: Unsigned,
    TakeSlot_NewUntypedCount<FU, U0>: Unsigned,

    TakeSlot_NewUntypedCount<FU, U0>: Add<U2>,
    Sum<TakeSlot_NewUntypedCount<FU, U0>, U2>: Unsigned,
{
    type OutputFlags = UInt<TakeSlot_Flags<FU, U0>, B1>;
    type NewUntypedCount = Sum<TakeSlot_NewUntypedCount<FU, U0>, U2>;
}

impl<Flags: Unsigned> Allocator<Flags> {
    pub fn alloc<Bits: Unsigned, FreeSlots: Unsigned, Role: CNodeRole>(
        mut self,
        dest_cnode: CNode<FreeSlots, Role>,
    ) -> (
        Cap<Untyped<Bits>, role::Local>,
        Allocator<TakeSlot_Flags<Flags, Bits>>,
        CNode<Diff<FreeSlots, TakeSlot_NewUntypedCount<Flags, Bits>>, Role>,
    )
    where
        Flags: _TakeSlot<Bits>,
        TakeSlot_Flags<Flags, Bits>: Unsigned,
        TakeSlot_NewUntypedCount<Flags, Bits>: Unsigned,

        FreeSlots: Sub<TakeSlot_NewUntypedCount<Flags, Bits>>,
        Diff<FreeSlots, TakeSlot_NewUntypedCount<Flags, Bits>>: Unsigned,
    {
        let pool_index = Bits::to_usize() - 4;
        match self.pool[pool_index] {
            Some(cptr) => {
                self.pool[pool_index] = None;
                //unimplemented!()

                (
                    Cap::<Untyped<Bits>, role::Local>::wrap_cptr(cptr),
                    Allocator {
                        pool: self.pool,
                        _flags: PhantomData,
                    },
                    // TODO
                    unsafe { transmute(dest_cnode) },
                )
            }
            None => unimplemented!(),
        }
    }
}

fn take_slot_compile_test() {
    type Bin000 = UInt<UInt<UInt<UTerm, B0>, B0>, B0>;
    type Bin001 = UInt<UInt<UInt<UTerm, B0>, B0>, B1>;
    type Bin010 = UInt<UInt<UInt<UTerm, B0>, B1>, B0>;
    type Bin011 = UInt<UInt<UInt<UTerm, B0>, B1>, B1>;
    type Bin100 = UInt<UInt<UInt<UTerm, B1>, B0>, B0>;
    type Bin101 = UInt<UInt<UInt<UTerm, B1>, B0>, B1>;
    type Bin110 = UInt<UInt<UInt<UTerm, B1>, B1>, B0>;
    type Bin111 = UInt<UInt<UInt<UTerm, B1>, B1>, B1>;

    let a_flags_res: Bin000 = unimplemented!();
    let a_flags: TakeSlot_Flags<Bin001, U0> = a_flags_res;
    let a_new_untyped_count_res: U0;
    let a_new_untyuped_count: TakeSlot_NewUntypedCount<Bin001, U0> = a_new_untyped_count_res;

    let b_flags_res: Bin010 = unimplemented!();
    let b_flags: TakeSlot_Flags<Bin011, U0> = b_flags_res;
    let b_new_untyped_count_res: U0;
    let b_new_untyuped_count: TakeSlot_NewUntypedCount<Bin011, U0> = b_new_untyped_count_res;

    let c_flags_res: Bin100 = unimplemented!();
    let c_flags: TakeSlot_Flags<Bin110, U1> = c_flags_res;
    let c_new_untyped_count_res: U0;
    let c_new_untyuped_count: TakeSlot_NewUntypedCount<Bin110, U1> = c_new_untyped_count_res;

    let d_flags_res: Bin011 = unimplemented!();
    let d_flags: TakeSlot_Flags<Bin100, U0> = d_flags_res;
    let d_new_untyped_count_res: U4;
    let d_new_untyuped_count: TakeSlot_NewUntypedCount<Bin100, U0> = d_new_untyped_count_res;

    let e_flags_res: Bin010 = unimplemented!();
    let e_flags: TakeSlot_Flags<Bin100, U1> = e_flags_res;
    let e_new_untyped_count_res: U2;
    let e_new_untyuped_count: TakeSlot_NewUntypedCount<Bin100, U1> = e_new_untyped_count_res;
}

fn allocator_compile_test() {
    type Bin1000000 = UInt<UInt<UInt<UInt<UInt<UInt<UInt<UTerm, B1>, B0>, B0>, B0>, B0>, B0>, B0>;
    type Bin0110000 = UInt<UInt<UInt<UInt<UInt<UInt<UInt<UTerm, B0>, B1>, B1>, B0>, B0>, B0>, B0>;

    // The allocator starts with a 6-bit untyped
    let ut: Cap<Untyped<U6>, role::Local> = unimplemented!();
    let allocator: Allocator<Bin1000000> = new_allocator(ut);
    let cnode: CNode<U10, role::Local> = unimplemented!();

    // after allocating a 4-bit untyped, there should be 1 4-bit and 1 5-bit left
    let (a, allocator, cnode): (Cap<Untyped<U4>, role::Local>, _, _) = allocator.alloc(cnode);
    let allocator: Allocator<Bin0110000> = allocator;

    // the allocation required two splits, so it consumed 4 cnode slots
    let cnode: CNode<U6, _> = cnode;
}
