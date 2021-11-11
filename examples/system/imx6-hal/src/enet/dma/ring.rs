use crate::enet::dma::{
    descriptor::{rx, tx, DescriptorSize},
    ring_entry::RingEntry,
    sealed, Rx, Tx,
};
use crate::enet::{Error, MinDescriptors, NumRxDescriptors, NumTxDescriptors};
use crate::pac::typenum::Unsigned;
use core::sync::atomic;

pub type RxDmaRing = DmaRing<Rx, { NumRxDescriptors::USIZE }>;
pub type TxDmaRing = DmaRing<Tx, { NumTxDescriptors::USIZE }>;

pub struct DmaRing<RxTx: sealed::RxTx, const N: usize> {
    next_entry: usize,
    pub(crate) entries: [RingEntry<RxTx>; N],
}

impl<RxTx: sealed::RxTx, const N: usize> DmaRing<RxTx, N> {
    pub fn new(entries: [RingEntry<RxTx>; N]) -> Result<Self, Error> {
        log::trace!("[enet] creating DMA ring len={}", N);
        if N < MinDescriptors::USIZE {
            Err(Error::NotEnoughDescriptors)
        } else {
            let ring = DmaRing {
                next_entry: 0,
                entries,
            };
            ring.check_contiguous()?;
            Ok(ring)
        }
    }

    pub(crate) fn check_contiguous(&self) -> Result<(), Error> {
        let mut last_desc_paddr = 0;
        for (idx, entry) in self.entries.iter().enumerate() {
            let desc_paddr = entry.desc.paddr();
            if idx > 0 && (desc_paddr != (last_desc_paddr + DescriptorSize::USIZE)) {
                log::error!(
                    "[enet] DMA ring not contiguous idx={}, prev=0x{:X}, cur=0x{:X}",
                    idx,
                    last_desc_paddr,
                    desc_paddr
                );
                return Err(Error::DmaRingMemoryNotContiguous);
            }
            last_desc_paddr = desc_paddr;
        }
        Ok(())
    }
}

impl<const N: usize> DmaRing<Rx, N> {
    const ERRS: rx::Status = rx::Status::from_bits_truncate(
        rx::Status::TR.bits()
            | rx::Status::OV.bits()
            | rx::Status::CR.bits()
            | rx::Status::NO.bits()
            | rx::Status::LG.bits(),
    );

    pub(crate) unsafe fn init(&mut self) {
        self.next_entry = 0;
        for entry in self.entries.iter_mut() {
            entry.init();
        }
        let last_desc = self.entries[N - 1].descriptor_mut();
        let status = last_desc.status();
        last_desc.set_status(status | rx::Status::W);
        atomic::fence(atomic::Ordering::SeqCst);
    }

    pub(crate) fn is_next_entry_empty(&self) -> bool {
        let desc = unsafe { self.entries[self.next_entry].descriptor() };
        let status = desc.status();
        status.contains(rx::Status::E)
    }

    pub(crate) fn consume_and_increment<F>(&mut self, mut f: F) -> usize
    where
        F: FnMut(&[u8]),
    {
        let desc = unsafe { self.entries[self.next_entry].descriptor_mut() };
        let len = desc.length() as usize;
        let status = desc.status();
        if status.contains(Self::ERRS) {
            log::warn!("[enet] rx status errors: {:?}", status);
        }
        let pkt = unsafe { self.entries[self.next_entry].packet() };
        f(&pkt[..len]);
        unsafe { self.entries[self.next_entry].complete() };
        self.next_entry += 1;
        if self.next_entry == N {
            self.next_entry = 0;
        }
        len
    }
}

impl<const N: usize> DmaRing<Tx, N> {
    pub(crate) unsafe fn init(&mut self) {
        self.next_entry = 0;
        for entry in self.entries.iter_mut() {
            entry.init();
        }
        let last_desc = self.entries[N - 1].descriptor_mut();
        let status = last_desc.status();
        last_desc.set_status(status | tx::Status::W);
        atomic::fence(atomic::Ordering::SeqCst);
    }

    pub(crate) fn is_next_entry_empty(&self) -> bool {
        let desc = unsafe { self.entries[self.next_entry].descriptor() };
        let status = desc.status();
        !status.contains(tx::Status::R)
    }

    // NOTE the size is checked by the caller
    pub(crate) fn fill_and_increment(&mut self, data: &[u8]) {
        let len = data.len();
        {
            let pkt = unsafe { &mut self.entries[self.next_entry].packet_mut()[..len] };
            pkt.copy_from_slice(data);
        }
        atomic::fence(atomic::Ordering::SeqCst);
        let desc = unsafe { self.entries[self.next_entry].descriptor_mut() };
        desc.set_length(len as _);
        let status = desc.status();
        desc.set_status(status | tx::Status::TC | tx::Status::L | tx::Status::R);
        self.next_entry += 1;
        if self.next_entry == N {
            self.next_entry = 0;
        }
    }
}
