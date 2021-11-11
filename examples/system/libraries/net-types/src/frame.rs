use core::fmt;
use typenum::*;

/// Default MTU size is 1,536 bytes
pub type MtuSize = Sum<U1024, U512>;

pub type IpcEthernetFrame = EthernetFrameBuffer<{ MtuSize::USIZE }>;

/// A Vec style octet buffer container, suitable for
/// imbuing with a smoltcp::wire::EthernetFrame structure
pub struct EthernetFrameBuffer<const N: usize> {
    len: usize,
    data: [u8; N],
}

impl<const N: usize> EthernetFrameBuffer<N> {
    pub fn new() -> Self {
        Self {
            len: N,
            data: [0; N],
        }
    }

    pub fn len(&self) -> usize {
        self.len
    }

    pub fn capacity(&self) -> usize {
        N
    }

    pub fn is_full(&self) -> bool {
        self.len == self.capacity()
    }

    pub fn is_empty(&self) -> bool {
        self.len == 0
    }

    pub fn truncate(&mut self, len: usize) {
        if len < self.len() {
            self.len = len;
        }
    }

    #[inline]
    pub fn as_slice(&self) -> &[u8] {
        &self.data[..self.len]
    }

    #[inline]
    pub fn as_mut_slice(&mut self) -> &mut [u8] {
        &mut self.data[..self.len]
    }
}

impl<const N: usize> AsRef<[u8]> for EthernetFrameBuffer<N> {
    fn as_ref(&self) -> &[u8] {
        self.as_slice()
    }
}

impl<const N: usize> AsMut<[u8]> for EthernetFrameBuffer<N> {
    fn as_mut(&mut self) -> &mut [u8] {
        self.as_mut_slice()
    }
}

impl<const N: usize> Default for EthernetFrameBuffer<N> {
    fn default() -> Self {
        Self::new()
    }
}

impl<const N: usize> fmt::Display for EthernetFrameBuffer<N> {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "EthernetFrameBuffer len={}", self.len())
    }
}
