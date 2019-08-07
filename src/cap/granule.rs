use core::marker::PhantomData;
use core::ops::Sub;

use typenum::*;

use crate::arch::{PageBits, G1, G2, G3, G4};
use crate::cap::{Cap, CapRangeDataReconstruction, CapType, CopyAliasable, LocalCap, Page};
use crate::pow::{Pow, _Pow};
use crate::{IfThenElse, _IfThenElse};

/// The type returned by the architecture specific implementations of
/// `determine_best_granule_fit`.
pub(crate) struct GranuleInfo {
    /// The seL4 type id for this granule.
    pub(crate) type_id: usize,
    /// This granule's size in bits (radix in seL4 parlance).
    pub(crate) size_bits: u8,
    /// How many of them do I need to do this?
    pub(crate) count: u16,
}

/// An abstract way of thinking about the leaves in paging structures
/// across architectures. A Granule can be a Page, a LargePage, a
/// Section, &c.
pub struct Granule<State: GranuleState> {
    /// The size of this granule in bits.
    pub(crate) size_bits: u8,
    /// The seL4 object id.
    pub(crate) type_id: usize,
    /// Is this granule mapped or unmapped and the state that goes
    /// along with that.
    pub(crate) state: State,
}

impl<State: GranuleState> Granule<State> {
    pub(crate) fn size_bytes(&self) -> usize {
        1 << self.size_bits
    }
}

impl<State: GranuleState> CapType for Granule<State> {}

impl<State: GranuleState> CapRangeDataReconstruction for Granule<State> {
    fn reconstruct(index: usize, seed_cap_data: &Self) -> Self {
        Granule {
            size_bits: seed_cap_data.size_bits,
            type_id: seed_cap_data.type_id,
            state: seed_cap_data
                .state
                .offset_by(index * seed_cap_data.size_bytes())
                .unwrap(),
        }
    }
}

impl<'a, State: GranuleState> From<&'a Granule<State>> for Granule<granule_state::Unmapped> {
    fn from(val: &'a Granule<State>) -> Self {
        Granule {
            type_id: val.type_id,
            size_bits: val.size_bits,
            state: granule_state::Unmapped {},
        }
    }
}

impl<State: GranuleState> From<LocalCap<Page<State>>> for LocalCap<Granule<State>> {
    fn from(page: LocalCap<Page<State>>) -> Self {
        Cap {
            cptr: page.cptr,
            cap_data: Granule {
                type_id: Page::<State>::TYPE_ID,
                size_bits: PageBits::U8,
                state: page.cap_data.state,
            },
            _role: PhantomData,
        }
    }
}

impl<State: GranuleState> CopyAliasable for Granule<State> {
    type CopyOutput = Granule<granule_state::Unmapped>;
}

pub trait GranuleState:
    private::SealedGranuleState + Copy + Clone + core::fmt::Debug + Sized + PartialEq
{
    fn offset_by(&self, bytes: usize) -> Option<Self>;
}

pub mod granule_state {
    use crate::cap::InternalASID;

    #[derive(Clone, Copy, Debug, PartialEq)]
    pub struct Mapped {
        pub(crate) vaddr: usize,
        pub(crate) asid: InternalASID,
    }
    impl super::GranuleState for Mapped {
        fn offset_by(&self, bytes: usize) -> Option<Self> {
            if let Some(b) = self.vaddr.checked_add(bytes) {
                Some(Mapped {
                    vaddr: b,
                    asid: self.asid,
                })
            } else {
                None
            }
        }
    }

    #[derive(Clone, Copy, Debug, PartialEq)]
    pub struct Unmapped;
    impl super::GranuleState for Unmapped {
        fn offset_by(&self, _bytes: usize) -> Option<Self> {
            Some(Unmapped)
        }
    }
}

mod private {
    pub trait SealedGranuleState {}
    impl SealedGranuleState for super::granule_state::Unmapped {}
    impl SealedGranuleState for super::granule_state::Mapped {}
}

pub(crate) trait IToUUnsafe: Integer {
    type Uint: Unsigned;
}

impl<U: Unsigned + NonZero> IToUUnsafe for PInt<U> {
    type Uint = U;
}

impl<U: Unsigned + NonZero> IToUUnsafe for NInt<U> {
    type Uint = U0;
}

impl IToUUnsafe for Z0 {
    type Uint = U0;
}

pub(crate) trait _GranuleSlotCount {
    type Count: Unsigned;
}

pub(crate) type GranuleSlotCount<U> = <U as _GranuleSlotCount>::Count;

impl<U: Unsigned + NonZero> _GranuleSlotCount for U
where
    // The incoming size must be NonZero; this is a requirement for
    // putting it into a positive int. More on that later.

    // We need to be able to subtract G1..G4 from U, forall U; some of
    // these results may be negative. Therefore we wrap a positiive
    // signed integer around U and do the subtraction from there
    // instead.
    PInt<U>: Sub<PInt<G1>>,
    <PInt<U> as Sub<PInt<G1>>>::Output: IToUUnsafe,

    PInt<U>: Sub<PInt<G2>>,
    <PInt<U> as Sub<PInt<G2>>>::Output: IToUUnsafe,

    PInt<U>: Sub<PInt<G3>>,
    <PInt<U> as Sub<PInt<G3>>>::Output: IToUUnsafe,

    PInt<U>: Sub<PInt<G4>>,
    <PInt<U> as Sub<PInt<G4>>>::Output: IToUUnsafe,

    // U must be comparable with G1..G4 and actually greater than G4,
    // the smallest granule.
    U: IsGreaterOrEqual<G1>,
    U: IsGreaterOrEqual<G2>,
    U: IsGreaterOrEqual<G3>,
    U: IsGreaterOrEqual<G4, Output = True>,

    // Okay, now the conditionals. The next three constraints allow us
    // to write the following algorithm:
    //
    // if U >= G1 then
    //   return U - G1
    // else
    //   if U >= G2 then
    //     return U - G2
    //   else
    //     if U >= G3 then
    //       return U - G3
    //     else
    //       return U - G4
    <U as IsGreaterOrEqual<G1>>::Output:
        _IfThenElse<<<PInt<U> as Sub<PInt<G1>>>::Output as IToUUnsafe>::Uint, GranuleSubCond2<U>>,

    <U as IsGreaterOrEqual<G2>>::Output:
        _IfThenElse<<<PInt<U> as Sub<PInt<G2>>>::Output as IToUUnsafe>::Uint, GranuleSubCond1<U>>,

    <U as IsGreaterOrEqual<G3>>::Output: _IfThenElse<
        <<PInt<U> as Sub<PInt<G3>>>::Output as IToUUnsafe>::Uint,
        <<PInt<U> as Sub<PInt<G4>>>::Output as IToUUnsafe>::Uint,
    >,

    GranuleCondExpr<U>: _Pow,

    GranuleCountExpr<U>: Unsigned,
{
    type Count = GranuleCountExpr<U>;
}

pub(crate) type GranuleSubCond1<U> = IfThenElse<
    <U as IsGreaterOrEqual<G3>>::Output,
    <<PInt<U> as Sub<PInt<G3>>>::Output as IToUUnsafe>::Uint,
    <<PInt<U> as Sub<PInt<G4>>>::Output as IToUUnsafe>::Uint,
>;

pub(crate) type GranuleSubCond2<U> = IfThenElse<
    <U as IsGreaterOrEqual<G2>>::Output,
    <<PInt<U> as Sub<PInt<G2>>>::Output as IToUUnsafe>::Uint,
    GranuleSubCond1<U>,
>;

// Now that we've done our subtraction, we need to use the result
// as an exponent to compute our slot count. Alotogether we're
// computing
// 2^x / 2^y == 2^(x-y).
pub(crate) type GranuleCondExpr<U> = IfThenElse<
    <U as IsGreaterOrEqual<G1>>::Output,
    <<PInt<U> as Sub<PInt<G1>>>::Output as IToUUnsafe>::Uint,
    GranuleSubCond2<U>,
>;

// This last one says the whole thing will ultimately give us an
// Unsigned, which is what we need to parameterize
// `CNodeSlots::alloc`.
pub(crate) type GranuleCountExpr<U> = Pow<
    IfThenElse<
        <U as IsGreaterOrEqual<G1>>::Output,
        <<PInt<U> as Sub<PInt<G1>>>::Output as IToUUnsafe>::Uint,
        IfThenElse<
            <U as IsGreaterOrEqual<G2>>::Output,
            <<PInt<U> as Sub<PInt<G2>>>::Output as IToUUnsafe>::Uint,
            IfThenElse<
                <U as IsGreaterOrEqual<G3>>::Output,
                <<PInt<U> as Sub<PInt<G3>>>::Output as IToUUnsafe>::Uint,
                <<PInt<U> as Sub<PInt<G4>>>::Output as IToUUnsafe>::Uint,
            >,
        >,
    >,
>;
