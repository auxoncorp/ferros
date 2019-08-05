use core::marker::PhantomData;

use selfe_sys::seL4_Yield;

use typenum::*;

use crate::error::*;
use crate::vspace::VSpaceError;

pub(crate) use crate::arch::userland::process::*;

mod standard;
pub use standard::*;

mod self_hosted;
pub use self_hosted::*;

pub type DefaultStackBitSize = U20;
pub type DefaultStackPageCount = op!((U1 << U20) / U4096);
pub type DefaultPrepareThreadCNodeSlots = op!(DefaultStackPageCount + U64);

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

pub fn yield_forever() -> ! {
    unsafe {
        loop {
            seL4_Yield();
        }
    }
}

#[derive(Debug)]
pub enum ProcessSetupError {
    ProcessParameterTooBigForStack,
    ProcessParameterHandoffSizeMismatch,
    NotEnoughCNodeSlots,
    VSpaceError(VSpaceError),
    SeL4Error(SeL4Error),
    ElfParseError(&'static str),
}

impl From<VSpaceError> for ProcessSetupError {
    fn from(e: VSpaceError) -> Self {
        ProcessSetupError::VSpaceError(e)
    }
}

impl From<SeL4Error> for ProcessSetupError {
    fn from(e: SeL4Error) -> Self {
        ProcessSetupError::SeL4Error(e)
    }
}
