use crate::{EthernetFrameBuffer, Ipv4Address, MtuSize, Port};
use core::fmt;
use typenum::Unsigned;

pub type IpcUdpTransmitBuffer = UdpTransmitBuffer<{ MtuSize::USIZE }>;

/// A UDP transmit buffer
pub struct UdpTransmitBuffer<const N: usize> {
    pub dst_addr: Ipv4Address,
    pub dst_port: Port,
    pub frame: EthernetFrameBuffer<N>,
}

impl<const N: usize> fmt::Display for UdpTransmitBuffer<N> {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(
            f,
            "UdpTransmitBuffer dst_addr={} dst_port={} len={}",
            self.dst_addr,
            self.dst_port,
            self.frame.len()
        )
    }
}
