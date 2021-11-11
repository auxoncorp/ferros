#![no_std]

use ferros::cap::{role, CNodeRole};
use ferros::userland::{Consumer1, Producer, RetypeForSetup};
use ferros::vspace::{shared_status, MappedMemoryRegion};
use imx6_hal::pac::gpt::{self, GPT};
use net_types::{EthernetAddress, IpcEthernetFrame, IpcUdpTransmitBuffer, Ipv4Address, MtuSize};
use static_assertions::const_assert;
use typenum::{op, Unsigned, U1, U12, U2};

/// Rx/Tx socket buffer size, 4K each, ~2 MTU/frames
pub type SocketBufferSizeBits = U12;
pub type SocketBufferSize = op!(U1 << SocketBufferSizeBits);
pub type RxTxSocketBufferSizeBits = op!(SocketBufferSizeBits + U1);
pub type RxTxSocketBufferSize = op!(U1 << RxTxSocketBufferSizeBits);

pub type MtuSize2x = op!(MtuSize * U2);
const_assert!(SocketBufferSize::USIZE >= MtuSize2x::USIZE);
pub type MtuSize4x = op!(MtuSize2x * U2);
const_assert!(RxTxSocketBufferSize::USIZE >= MtuSize4x::USIZE);

#[repr(C)]
pub struct ProcParams<Role: CNodeRole> {
    /// General purpose timer provides a time domain
    /// and periodic service interrupt
    pub gpt: GPT,

    /// Consumer of Ethernet frames from a L2 driver
    pub frame_consumer: Consumer1<Role, IpcEthernetFrame>,

    /// Producer of Ethernet frames destined to a L2 driver
    pub frame_producer: Producer<Role, IpcEthernetFrame>,

    /// The event consumer handles:
    /// - GPT IRQ notification events (via Waker)
    /// - UDP transmit buffers
    pub event_consumer: Consumer1<Role, IpcUdpTransmitBuffer, gpt::Irq>,

    /// Memory for the socket buffers, split in half for rx and tx by the driver
    pub socket_buffer_mem: MappedMemoryRegion<RxTxSocketBufferSizeBits, shared_status::Exclusive>,

    /// Hardware MAC address
    pub mac_addr: EthernetAddress,

    /// IPv4 address
    pub ip_addr: Ipv4Address,
}

impl RetypeForSetup for ProcParams<role::Local> {
    type Output = ProcParams<role::Child>;
}
