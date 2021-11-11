#![no_std]

use ferros::cap::{role, CNodeRole};
use ferros::userland::{Consumer1, Producer, RetypeForSetup};
use ferros::vspace::{shared_status, MappedMemoryRegion};
use imx6_hal::pac::{
    enet::{self, ENET},
    typenum::{op, U1, U16},
};
use net_types::{EthernetAddress, IpcEthernetFrame};

/// Expected badge value on IRQ notifications
pub type IrqBadgeBits = enet::Irq;

/// For the ENET Ethernet driver DMA descriptors and packets
/// 1 page for the descriptors, split in half for rx/tx, up to 256 for each
/// 1 page for tx packets (2)
/// 16 pages for rx packets (32)
pub type EthDmaMemSizeInBits = U16;
pub type EthDmaMemSizeInBytes = op!(U1 << EthDmaMemSizeInBits);

#[repr(C)]
pub struct ProcParams<Role: CNodeRole> {
    /// ENET device
    pub enet: ENET,

    /// Consumer of Ethernet frames to be sent out on the ENET egress, in
    /// addition to IRQ notification wakeup events
    pub consumer: Consumer1<Role, IpcEthernetFrame, enet::Irq>,

    /// Producer of Ethernet frames received from the ENET ingress
    pub producer: Producer<Role, IpcEthernetFrame>,

    /// DMA-able memory for use by the Ethernet Rx/Tx descriptors and packets.
    ///
    /// NOTE: currently expects to be mapped *not* cacheable
    pub dma_mem: MappedMemoryRegion<EthDmaMemSizeInBits, shared_status::Exclusive>,

    /// Hardware MAC address
    pub mac_addr: EthernetAddress,
}

impl RetypeForSetup for ProcParams<role::Local> {
    type Output = ProcParams<role::Child>;
}
