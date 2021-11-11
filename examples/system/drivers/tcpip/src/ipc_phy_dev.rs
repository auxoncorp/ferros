use ferros::cap::role;
use ferros::userland::{Consumer1, Producer};
use net_types::{IpcEthernetFrame, MtuSize};
use smoltcp::phy::{Checksum, Device, DeviceCapabilities, RxToken, TxToken};
use smoltcp::time::Instant;
use smoltcp::Error;
use typenum::Unsigned;

/// An interface for sending and receiving raw network frames
/// over ferros IPC
pub struct IpcPhyDevice {
    pub consumer: Consumer1<role::Local, IpcEthernetFrame>,
    pub producer: Producer<role::Local, IpcEthernetFrame>,
}

impl<'a> Device<'a> for IpcPhyDevice {
    type RxToken = IpcPhyRxToken;
    type TxToken = IpcPhyTxToken<'a>;

    fn receive(&'a mut self) -> Option<(Self::RxToken, Self::TxToken)> {
        if let Some(data) = self.consumer.poll() {
            let rx = IpcPhyRxToken(data);
            let tx = IpcPhyTxToken {
                producer: &mut self.producer,
            };
            Some((rx, tx))
        } else {
            None
        }
    }

    fn transmit(&'a mut self) -> Option<Self::TxToken> {
        Some(IpcPhyTxToken {
            producer: &mut self.producer,
        })
    }

    fn capabilities(&self) -> DeviceCapabilities {
        let mut caps = DeviceCapabilities::default();

        // Our max MTU size
        caps.max_transmission_unit = MtuSize::USIZE;

        // Limit bursts to 1
        caps.max_burst_size = Some(1);

        // Verify checksum when receiving and compute checksum when sending
        caps.checksum.ipv4 = Checksum::Both;
        caps.checksum.udp = Checksum::Both;
        caps.checksum.tcp = Checksum::Both;
        caps.checksum.icmpv4 = Checksum::Both;

        caps
    }
}

pub struct IpcPhyRxToken(IpcEthernetFrame);

impl RxToken for IpcPhyRxToken {
    fn consume<R, F>(mut self, timestamp: Instant, f: F) -> Result<R, Error>
    where
        F: FnOnce(&mut [u8]) -> Result<R, Error>,
    {
        log::trace!(
            "[ipc-phy-dev] [{}] Receiving {} from L2 driver",
            timestamp,
            self.0
        );
        let result = f(self.0.as_mut_slice());
        result
    }
}

pub struct IpcPhyTxToken<'a> {
    producer: &'a mut Producer<role::Local, IpcEthernetFrame>,
}

impl<'a> TxToken for IpcPhyTxToken<'a> {
    fn consume<R, F>(self, timestamp: Instant, len: usize, f: F) -> Result<R, Error>
    where
        F: FnOnce(&mut [u8]) -> Result<R, Error>,
    {
        let mut data = IpcEthernetFrame::new();
        data.truncate(len);

        log::trace!(
            "[ipc-phy-dev] [{}] Sending {} to L2 driver",
            timestamp,
            data,
        );

        let result = f(data.as_mut_slice());

        if result.is_ok() && self.producer.send(data).is_err() {
            // Drop the data if the queue is full
            log::warn!(
                "[ipc-phy-dev] [{}] Rejected sending IpcEthernetFrame data to L2 driver",
                timestamp
            );
            return Err(Error::Exhausted);
        }

        result
    }
}
