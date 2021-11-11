use self::dma::descriptor::DescriptorSize;
use self::dma::ring::{RxDmaRing, TxDmaRing};
use self::dma::ring_entry::{RxRingEntry, TxRingEntry};
use self::uncached_memory_region::{Error as MemRegionError, UncachedMemoryRegion};
use crate::asm;
use imx6_devices::{enet::*, typenum::*};
use net_types::EthernetAddress;
use static_assertions::const_assert_eq;

pub mod dma;
pub mod uncached_memory_region;

/// Frame length is 1,518 bytes
pub type FrameLength = Sum<U1024, U494>;

/// MTU size is 1,536 bytes, this is also the size of each packet buffer
pub type MtuSize = Sum<U1024, U512>;
const_assert_eq!(MtuSize::USIZE, net_types::MtuSize::USIZE);

/// Number of receive descriptors, up to 128 supported
pub type NumRxDescriptors = U32;

/// Number of trasmit descriptors, up to 128 supported
pub type NumTxDescriptors = U2;

/// Need at least 2 descriptors (both rx and tx)
pub type MinDescriptors = U2;

pub const ENET_FREQ_HZ: u32 = 125_000_000;
pub const MDC_FREQ_HZ: u32 = 20_000_000;

/// Pause duration field when sending pause frames
type PauseDuration = U32;

/// Number of bytes in buffer before transmission begins
type StrFwdBytes = U128;
type StrFwd = op!(StrFwdBytes / U64);

/// Fixed magic opcode used when sending pause frames
type PauseOpcode = U1;

/// TX inter-packet gap between 8 and 27 bytes
type TxIpgSize = U8;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Error {
    ExhaustedResource,
    NotEnoughDescriptors,
    DmaRingMemoryNotContiguous,
    TransmitBufferTooBig,
    MemoryRegion(MemRegionError),
}

impl From<MemRegionError> for Error {
    fn from(e: MemRegionError) -> Self {
        Error::MemoryRegion(e)
    }
}

pub struct Enet {
    enet: ENET,
    mac: EthernetAddress,
    rx_ring: RxDmaRing,
    tx_ring: TxDmaRing,
}

impl Enet {
    pub fn new(
        enet: ENET,
        mac: EthernetAddress,
        mut desc_mem: UncachedMemoryRegion,
        mut packet_mem: UncachedMemoryRegion,
    ) -> Result<Self, Error> {
        log::trace!("[enet] new MAC={}", mac);

        let rx_total_desc_size = NumRxDescriptors::USIZE * DescriptorSize::USIZE;
        let tx_total_desc_size = NumTxDescriptors::USIZE * DescriptorSize::USIZE;
        if desc_mem.size() < (rx_total_desc_size + tx_total_desc_size) {
            log::error!("[enet] Descriptor memory too small");
            return Err(Error::ExhaustedResource);
        }

        let rx_total_pkt_size = NumRxDescriptors::USIZE * MtuSize::USIZE;
        let tx_total_pkt_size = NumTxDescriptors::USIZE * MtuSize::USIZE;
        if packet_mem.size() < (rx_total_pkt_size + tx_total_pkt_size) {
            log::error!("[enet] Packet memory too small");
            return Err(Error::ExhaustedResource);
        }

        // Split up descriptor memory, tx off the tail end
        let mut tx_desc_mem = desc_mem.split_off(desc_mem.size() - tx_total_desc_size)?;
        debug_assert_eq!(tx_desc_mem.size(), tx_total_desc_size);

        desc_mem.shrink_to(rx_total_desc_size)?;
        let mut rx_desc_mem = desc_mem;
        debug_assert_eq!(rx_desc_mem.size(), rx_total_desc_size);

        // Split up the packet memory, tx off the tail end
        let mut tx_pkt_mem = packet_mem.split_off(packet_mem.size() - tx_total_pkt_size)?;
        debug_assert_eq!(tx_pkt_mem.size(), tx_total_pkt_size);

        packet_mem.shrink_to(rx_total_pkt_size)?;
        let mut rx_pkt_mem = packet_mem;
        debug_assert_eq!(rx_pkt_mem.size(), rx_total_pkt_size);

        let tx_entries: [TxRingEntry; NumTxDescriptors::USIZE] =
            array_init::try_array_init(|_i| {
                // Split off the head of the region so it's contiguous
                let desc = if tx_desc_mem.size() > DescriptorSize::USIZE {
                    tx_desc_mem.split(DescriptorSize::USIZE)?
                } else {
                    tx_desc_mem
                };
                let pkt = if tx_pkt_mem.size() > MtuSize::USIZE {
                    tx_pkt_mem.split(MtuSize::USIZE)?
                } else {
                    tx_pkt_mem
                };
                TxRingEntry::new(desc, pkt)
            })?;

        let rx_entries: [RxRingEntry; NumRxDescriptors::USIZE] =
            array_init::try_array_init(|_i| {
                // Split off the head of the region so it's contiguous
                let desc = if rx_desc_mem.size() > DescriptorSize::USIZE {
                    rx_desc_mem.split(DescriptorSize::USIZE)?
                } else {
                    rx_desc_mem
                };
                let pkt = if rx_pkt_mem.size() > MtuSize::USIZE {
                    rx_pkt_mem.split(MtuSize::USIZE)?
                } else {
                    rx_pkt_mem
                };
                RxRingEntry::new(desc, pkt)
            })?;

        let mut tx_ring = TxDmaRing::new(tx_entries)?;
        let mut rx_ring = RxDmaRing::new(rx_entries)?;
        unsafe {
            tx_ring.init();
            rx_ring.init();
        }

        Ok(Enet {
            enet,
            mac,
            rx_ring,
            tx_ring,
        })
    }

    pub fn init(&mut self) {
        log::trace!("[enet] init");

        // TODO - need to configure IOMUX
        // Initialise ethernet pins, also does a PHY reset

        // Set MAC and pause frame type field
        self.set_mac();

        // Configure pause frames (continues into MAC registers
        self.enet.opd.modify(
            OpcodePauseDuration::Duration::Field::checked::<PauseDuration>()
                + OpcodePauseDuration::Opcode::Field::checked::<PauseOpcode>(),
        );

        // TX inter-packet gap
        self.enet
            .tipg
            .modify(TxIpg::Ipg::Field::checked::<TxIpgSize>());

        //  Transmit FIFO Watermark register - store and forward
        self.enet.tfwr.modify(
            TxFifoWatermark::FifoWrite::Field::checked::<StrFwd>()
                + TxFifoWatermark::StoreAndFowardEnable::Set,
        );

        // Do not forward frames with errors
        self.enet.racc.modify(
            RxAccelFnConfig::PadRem::Clear
                + RxAccelFnConfig::IpDiscard::Clear
                + RxAccelFnConfig::ProtoDiscard::Clear
                + RxAccelFnConfig::LineDiscard::Set
                + RxAccelFnConfig::Shift16::Clear,
        );

        // DMA descriptors
        unsafe {
            self.enet
                .tdsr
                .write(self.tx_ring.entries[0].desc.paddr() as u32 & 0xFFFF_FFF8);
            self.enet
                .rdsr
                .write(self.rx_ring.entries[0].desc.paddr() as u32 & 0xFFFF_FFF8);
        }
        self.enet
            .mrbr
            .modify(MaxRxBufferSize::BufSize::Field::checked::<MtuSize>());

        // Receive control - Set frame length and RGMII mode
        self.enet.rcr.modify(
            RxControl::Loop::Clear
                + RxControl::Drt::Clear
                + RxControl::MiiMode::Set
                + RxControl::Prom::Clear
                + RxControl::BcastReject::Clear
                + RxControl::FlowControlEnable::Clear
                + RxControl::RgmiiEnable::Set
                + RxControl::RmiiMode::Clear
                + RxControl::Rmii10t::Clear
                + RxControl::PadEnable::Clear
                + RxControl::PauseForward::Clear
                + RxControl::CrcForward::Clear
                + RxControl::CfEnable::Clear
                + RxControl::MaxFrameLength::Field::checked::<FrameLength>()
                + RxControl::Nlc::Clear
                + RxControl::Grs::Clear,
        );

        // Transmit control - Full duplex mode
        self.enet.tcr.modify(
            TxControl::Gts::Clear
                + TxControl::FdEnable::Set
                + TxControl::TfcPause::Clear
                + TxControl::RfcPause::Clear
                + TxControl::AddrSelect::Field::checked::<U0>()
                + TxControl::AddrIns::Clear
                + TxControl::CrcForward::Clear,
        );

        self.set_crc_strip(true);
        self.set_promiscuous_mode(false);

        // Connect the phy to the ethernet controller
        // TODO
        // all the MDIO/PHY/link stuff

        //
        // Initialize FEC, ENET1 has an MDIO interface
        // TODO

        self.set_duplex_speed();

        // Start the controller, all interrupts are still masked here
        self.enable();

        // Enable Rx descriptor ring
        self.enet.rdar.modify(RxDescActive::RxDescActive::Set);

        // Ensure no unused interrupts are pending
        self.enet.eir.modify(
            InterruptEvent::TsTimer::Set
                + InterruptEvent::TsAvail::Set
                + InterruptEvent::Wakeup::Set
                + InterruptEvent::PayloadRxErr::Set
                + InterruptEvent::TxFifoUnderrun::Set
                + InterruptEvent::CollisionRetryLimit::Set
                + InterruptEvent::LateCollision::Set
                + InterruptEvent::BusErr::Clear
                + InterruptEvent::Mii::Set
                + InterruptEvent::RxBuffer::Set
                + InterruptEvent::RxFrame::Clear
                + InterruptEvent::TxBuffer::Set
                + InterruptEvent::TxFrame::Clear
                + InterruptEvent::GStopComple::Set
                + InterruptEvent::BTxErr::Set
                + InterruptEvent::BRxErr::Set,
        );

        // Enable interrupts
        self.enet
            .eimr
            .modify(InterruptMask::RxFrame::Set + InterruptMask::BusErr::Set);
    }

    // NOTE: after reset, the caller should
    // * configure the ENET clock after reset to ENET_FREQ_HZ
    // * configure MDIO clock frequency to MDC_FREQ_HZ
    pub fn reset(&mut self) {
        log::trace!("[enet] reset");
        unsafe { self.enet.ecr.write(0) };
        self.enet.ecr.modify(Control::Reset::Set);
        while self.enet.ecr.is_set(Control::Reset::Set) {
            asm::nop();
        }

        // Little-endian mode, legacy descriptors
        self.enet
            .ecr
            .modify(Control::DescByteSwap::Set + Control::Enable1588::Clear);

        // Clear and mask interrupts
        unsafe {
            self.enet.eimr.write(0x0000_0000);
            self.enet.eir.write(0xFFFF_FFFF);
        }

        self.clear_mib();

        // Descriptor group and individual hash tables - Not changed on reset
        unsafe {
            self.enet.iaur.write(0);
            self.enet.ialr.write(0);
            self.enet.gaur.write(0);
            self.enet.galr.write(0);
        }
    }

    pub fn ack_irqs(&mut self) -> bool {
        let irqs = self.enet.eir.extract();
        self.enet
            .eir
            .modify(InterruptEvent::BusErr::Set + InterruptEvent::RxFrame::Set);

        if irqs.is_set(InterruptEvent::BusErr::Set) {
            log::warn!("[enet] BUS error");
        }

        irqs.is_set(InterruptEvent::RxFrame::Set)
    }

    pub fn receive<F>(&mut self, mut f: F) -> usize
    where
        F: FnMut(&[u8]),
    {
        if !self.rx_ring.is_next_entry_empty() {
            // Enable Rx descriptor ring
            self.enet.rdar.modify(RxDescActive::RxDescActive::Set);
            self.rx_ring.consume_and_increment(&mut f)
        } else {
            0
        }
    }

    // NOTE: fire and forget, does not block until completion
    pub fn transmit(&mut self, data: &[u8]) -> Result<(), Error> {
        if data.len() > MtuSize::USIZE {
            Err(Error::TransmitBufferTooBig)
        } else {
            while !self.tx_ring.is_next_entry_empty() {
                asm::nop();
            }
            self.tx_ring.fill_and_increment(data);

            // Enable Tx descriptor ring
            self.enet.tdar.modify(TxDescActive::TxDescActive::Set);
            Ok(())
        }
    }

    fn set_crc_strip(&mut self, enable: bool) {
        log::trace!("[enet] CRC stripping {}", if enable { "ON" } else { "OFF" });
        if enable {
            self.enet.rcr.modify(RxControl::CrcForward::Set);
        } else {
            self.enet.rcr.modify(RxControl::CrcForward::Clear);
        }
    }

    fn set_promiscuous_mode(&mut self, enable: bool) {
        log::trace!(
            "[enet] promiscuous mode {}",
            if enable { "ON" } else { "OFF" }
        );
        if enable {
            self.enet.rcr.modify(RxControl::Prom::Set);
        } else {
            self.enet.rcr.modify(RxControl::Prom::Clear);
        }
    }

    // Harded coded to full-duplexx 100T, RGMII mode
    fn set_duplex_speed(&mut self) {
        self.enet.ecr.modify(Control::Speed::Clear);
        self.enet.rcr.modify(
            RxControl::RgmiiEnable::Set
                + RxControl::RmiiMode::Clear
                + RxControl::MiiMode::Set
                + RxControl::Rmii10t::Clear
                + RxControl::Drt::Clear,
        );
    }

    fn enable(&mut self) {
        self.enet.ecr.modify(Control::Enable::Set);
    }

    fn set_mac(&mut self) {
        self.enet.palr.modify(
            PhysicalAddressLower::Octet0::Field::new(self.mac.0[0].into()).unwrap()
                + PhysicalAddressLower::Octet1::Field::new(self.mac.0[1].into()).unwrap()
                + PhysicalAddressLower::Octet2::Field::new(self.mac.0[2].into()).unwrap()
                + PhysicalAddressLower::Octet3::Field::new(self.mac.0[3].into()).unwrap(),
        );
        self.enet.paur.modify(
            PhysicalAddressUpper::Octet4::Field::new(self.mac.0[4].into()).unwrap()
                + PhysicalAddressUpper::Octet5::Field::new(self.mac.0[5].into()).unwrap()
                + PhysicalAddressUpper::Type::Field::new(0x8808).unwrap(),
        );
    }

    fn clear_mib(&mut self) {
        self.enet.mibc.modify(MibControl::Disable::Set);
        while !self.enet.mibc.is_set(MibControl::Idle::Set) {
            asm::nop();
        }
        self.enet.mibc.modify(MibControl::Clear::Set);
        while !self.enet.mibc.is_set(MibControl::Idle::Set) {
            asm::nop();
        }
        self.enet
            .mibc
            .modify(MibControl::Disable::Clear + MibControl::Clear::Clear);
    }
}
