//! 2^n for typenum
use core::ops::Sub;
use typenum::operator_aliases::Diff;
use typenum::{Bit, UInt, UTerm, Unsigned, B0, B1, U1, U2};

pub trait _Pow {
    type Output;
}

// 2 ^ 0 = 1
impl _Pow for UTerm {
    type Output = U1;
}

// 2 ^ 1 = 2
impl _Pow for UInt<UTerm, B1> {
    type Output = U2;
}

// 2 ^ 0 = 1 (crazy version)
impl _Pow for UInt<UTerm, B0> {
    type Output = U1;
}

impl<U: Unsigned, BA: Bit, BB: Bit> _Pow for UInt<UInt<U, BB>, BA>
where
    Self: Sub<U1>,
    Diff<Self, U1>: _Pow,
{
    type Output = UInt<<Diff<Self, U1> as _Pow>::Output, B0>;
}

// shortcut
pub type Pow<A> = <A as _Pow>::Output;
