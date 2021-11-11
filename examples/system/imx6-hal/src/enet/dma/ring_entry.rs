use crate::enet::dma::{
    descriptor::{rx, tx, DescriptorSize},
    sealed, Rx, Tx,
};
use crate::enet::uncached_memory_region::UncachedMemoryRegion;
use crate::enet::{Error, MtuSize};
use crate::pac::typenum::Unsigned;
use core::{marker::PhantomData, sync::atomic};

pub type RxRingEntry = RingEntry<Rx>;
pub type TxRingEntry = RingEntry<Tx>;

pub struct RingEntry<RxTx: sealed::RxTx> {
    pub(crate) desc: UncachedMemoryRegion,
    pub(crate) pkt: UncachedMemoryRegion,
    _role: PhantomData<RxTx>,
}

impl<RxTx: sealed::RxTx> RingEntry<RxTx> {
    pub fn new(desc: UncachedMemoryRegion, pkt: UncachedMemoryRegion) -> Result<Self, Error> {
        if desc.size() < DescriptorSize::USIZE {
            log::error!("[enet] ring entry descriptor memory too small");
            Err(Error::ExhaustedResource)
        } else if pkt.size() < MtuSize::USIZE {
            log::error!("[enet] ring entry packet memory too small");
            Err(Error::ExhaustedResource)
        } else {
            Ok(RingEntry {
                desc,
                pkt,
                _role: PhantomData,
            })
        }
    }

    pub(crate) unsafe fn packet(&self) -> &[u8] {
        self.pkt.as_slice::<u8>(self.pkt.size())
    }

    pub(crate) unsafe fn packet_mut(&mut self) -> &mut [u8] {
        self.pkt.as_mut_slice::<u8>(self.pkt.size())
    }
}

impl RingEntry<Rx> {
    pub(crate) unsafe fn init(&mut self) {
        log::trace!(
            "[enet] init rx ring entry, descriptor={}, packet={}",
            self.desc,
            self.pkt
        );
        let pkt = self.pkt.as_mut_slice::<u8>(self.pkt.size());
        pkt.fill(0);

        let desc = &mut *self.desc.as_mut_ptr::<rx::Descriptor>();
        desc.zero();
        desc.set_address(self.pkt.paddr() as _);
        desc.set_status(rx::Status::E);

        atomic::fence(atomic::Ordering::SeqCst);
    }

    pub(crate) unsafe fn complete(&mut self) {
        let desc = &mut *self.desc.as_mut_ptr::<rx::Descriptor>();
        desc.set_length(0);
        atomic::fence(atomic::Ordering::SeqCst);
        let status = desc.status();
        desc.set_status((status & rx::Status::W) | rx::Status::E);
    }

    pub(crate) unsafe fn descriptor(&self) -> &rx::Descriptor {
        &*self.desc.as_ptr::<rx::Descriptor>()
    }

    pub(crate) unsafe fn descriptor_mut(&mut self) -> &mut rx::Descriptor {
        &mut *self.desc.as_mut_ptr::<rx::Descriptor>()
    }
}

impl RingEntry<Tx> {
    pub(crate) unsafe fn init(&mut self) {
        log::trace!(
            "[enet] init tx ring entry, descriptor={}, packet={}",
            self.desc,
            self.pkt
        );
        let pkt = self.pkt.as_mut_slice::<u8>(self.pkt.size());
        pkt.fill(0);

        let desc = &mut *self.desc.as_mut_ptr::<tx::Descriptor>();
        desc.zero();
        desc.set_address(self.pkt.paddr() as _);

        atomic::fence(atomic::Ordering::SeqCst);
    }

    pub(crate) unsafe fn descriptor(&self) -> &tx::Descriptor {
        &*self.desc.as_ptr::<tx::Descriptor>()
    }

    pub(crate) unsafe fn descriptor_mut(&mut self) -> &mut tx::Descriptor {
        &mut *self.desc.as_mut_ptr::<tx::Descriptor>()
    }
}
