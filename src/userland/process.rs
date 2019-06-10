use core::cmp;
use core::marker::PhantomData;
use core::mem::{self, size_of};
use core::ptr;

use selfe_sys::*;

pub(crate) use crate::arch::userland::process::*;

// TODO - consider renaming for clarity
pub trait RetypeForSetup: Sized + Send + Sync {
    type Output: Sized + Send + Sync;
}

pub type SetupVer<X> = <X as RetypeForSetup>::Output;

/// A helper zero-sized struct that forces structures
/// which have a field of its type to not auto-implement
/// core::marker::Send or core::marker::Sync.
///
/// Using this technique allows us to avoid a presently unstable
/// feature, `optin_builtin_traits` to explicitly opt-out of
/// implementing Send and Sync.
pub(crate) struct NeitherSendNorSync(PhantomData<*const ()>);

impl core::default::Default for NeitherSendNorSync {
    fn default() -> Self {
        NeitherSendNorSync(PhantomData)
    }
}
