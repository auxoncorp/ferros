#![no_std]

use core::fmt;
use ferros::cap::{role, CNodeRole};
use ferros::userland::{Caller, Responder, RetypeForSetup};
use ferros::vspace::{shared_status, MappedMemoryRegion};
use heapless::String;
use imx6_hal::pac::{
    ecspi1::ECSPI1,
    gpio::GPIO3,
    typenum::{op, U1, U12},
};
pub use tickv::{success_codes::SuccessCode, ErrorCode};

pub const MAX_KEY_SIZE: usize = 32;
pub type Key = String<MAX_KEY_SIZE>;

pub const MAX_VALUE_SIZE: usize = 256;
pub type Value = String<MAX_VALUE_SIZE>;

#[derive(Debug, Clone, PartialEq)]
pub enum Request {
    AppendKey(Key, Value),
    Get(Key),
    InvalidateKey(Key),
    GarbageCollect,
}

impl fmt::Display for Request {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Request::AppendKey(k, v) => write!(f, "AppendKey({}, {})", k.as_str(), v.as_str()),
            Request::Get(k) => write!(f, "Get({})", k.as_str()),
            Request::InvalidateKey(k) => write!(f, "InvalidateKey({})", k.as_str()),
            Request::GarbageCollect => write!(f, "GarbageCollect"),
        }
    }
}

#[allow(clippy::large_enum_variant)]
#[derive(Debug, Clone, PartialEq)]
pub enum Response {
    KeyAppended(SuccessCode),
    Value(Value),
    KeyInvalidated(SuccessCode),
    GarbageCollected(usize),
}

impl fmt::Display for Response {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Response::KeyAppended(sc) => write!(f, "KeyAppended({:?})", sc),
            Response::Value(v) => write!(f, "Value({})", v.as_str()),
            Response::KeyInvalidated(sc) => write!(f, "KeyInvalidated({:?})", sc),
            Response::GarbageCollected(size) => write!(f, "GarbageCollected({} bytes freed)", size),
        }
    }
}

/// 4K buffer for persistent storage in flash (1 sector)
pub type StorageBufferSizeBits = U12;
pub type StorageBufferSizeBytes = op! { U1 << StorageBufferSizeBits };

/// 4K scratchpad buffer
pub type ScratchpadBufferSizeBits = U12;
pub type ScratchpadBufferSizeBytes = op! { U1 << ScratchpadBufferSizeBits };

#[repr(C)]
pub struct ProcParams<Role: CNodeRole> {
    pub spi: ECSPI1,
    pub gpio3: GPIO3,
    pub iomux_caller: Caller<iomux::Request, iomux::Response, Role>,
    pub responder: Responder<Request, Result<Response, ErrorCode>, Role>,
    pub storage_buffer: MappedMemoryRegion<StorageBufferSizeBits, shared_status::Exclusive>,
    pub scratchpad_buffer: MappedMemoryRegion<ScratchpadBufferSizeBits, shared_status::Exclusive>,
}

impl RetypeForSetup for ProcParams<role::Local> {
    type Output = ProcParams<role::Child>;
}
