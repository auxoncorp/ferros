#![no_std]
#![recursion_limit = "256"]
#![feature(proc_macro_hygiene)]

extern crate arrayvec;
extern crate generic_array;
extern crate selfe_sys;
extern crate typenum;

extern crate cross_queue;
extern crate smart_alloc;

#[macro_use]
pub mod debug;

pub mod alloc;
pub mod arch;
pub mod bootstrap;
pub mod cap;
pub mod error;
pub mod pow;
#[cfg(feature = "test_support")]
pub mod test_support;
pub mod userland;
pub mod vspace;

/// Type-level if/else.
pub trait _IfThenElse<A, B>: typenum::Bit {
    type Output;
}

impl<R, L> _IfThenElse<R, L> for typenum::True {
    type Output = R;
}

impl<R, L> _IfThenElse<R, L> for typenum::False {
    type Output = L;
}

/// Typenum-style sugar for using if/else at the type level.
pub type IfThenElse<C, A, B> = <C as _IfThenElse<A, B>>::Output;
