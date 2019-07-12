//! A tiny first-chance allocator for the untyped capabilities sel4's BOOTINFO.
//! This one doesn't split anything; it just hands out the smallest untyped item
//! that's big enough for the request.
use core::fmt::{Debug, Error as FmtError, Formatter};
use core::marker::PhantomData;

use selfe_sys::seL4_BootInfo;

use crate::arch::MaxUntypedSize as MaxUntypedSizeBits;
use crate::arch::MinUntypedSize as MinUntypedSizeBits;
use crate::cap::{
    memory_kind, role, Cap, LocalCap, PhantomCap, Untyped, WCNodeSlotsData, WUntyped,
    WUntypedSplitError,
};
use crate::pow::Pow;
use arrayvec::ArrayVec;
use typenum::{Unsigned, U2};

pub type MIN_UNTYPED_SIZE_BYTES = Pow<MinUntypedSizeBits>;
pub type MAX_UNTYPED_SIZE_BYTES = Pow<MaxUntypedSizeBits>;

// TODO - pull from configs
pub const MAX_INIT_UNTYPED_ITEMS: usize = 256;

#[derive(Debug)]
pub enum Error {
    InvalidBootInfoCapability,
    UntypedSizeOutOfRange,
    TooManyDeviceUntypeds,
    TooManyGeneralUntypeds,
}

/// Use `BootInfo` to bootstrap both the device and general allocators.
pub fn bootstrap_allocators(
    bootinfo: &'static seL4_BootInfo,
) -> Result<(Allocator, DeviceAllocator), Error> {
    let mut general_uts = ArrayVec::new();
    let mut device_uts = ArrayVec::new();

    for i in 0..(bootinfo.untyped.end - bootinfo.untyped.start) {
        let cptr = (bootinfo.untyped.start + i) as usize;
        let ut = &bootinfo.untypedList[i as usize];
        if ut.isDevice == 1 {
            match device_uts.try_push(Cap {
                cptr,
                cap_data: WUntyped {
                    size_bits: ut.sizeBits,
                    kind: memory_kind::Device { paddr: ut.paddr },
                },
                _role: PhantomData,
            }) {
                Ok(()) => (),
                Err(_) => return Err(Error::TooManyDeviceUntypeds),
            }
        } else {
            match general_uts.try_push(Cap {
                cptr,
                cap_data: WUntyped {
                    size_bits: ut.sizeBits,
                    kind: memory_kind::General {},
                },
                _role: PhantomData,
            }) {
                Ok(()) => (),
                Err(_) => return Err(Error::TooManyGeneralUntypeds),
            }
        }
    }
    Ok((
        Allocator { items: general_uts },
        DeviceAllocator {
            untypeds: device_uts,
        },
    ))
}

/// An allocator for general purpose memory.
pub struct Allocator {
    items: ArrayVec<[LocalCap<WUntyped<memory_kind::General>>; MAX_INIT_UNTYPED_ITEMS]>,
}

impl Debug for Allocator {
    fn fmt(&self, f: &mut Formatter) -> Result<(), FmtError> {
        f.write_str("Allocator { items:")?;
        for i in &self.items {
            write!(f, "\ncptr: {}, size_bits: {}", i.cptr, i.size_bits()).unwrap();
        }
        f.write_str("\n }")
    }
}

impl Allocator {
    pub fn bootstrap(bootinfo: &'static seL4_BootInfo) -> Result<Allocator, Error> {
        let (alloc, _) = bootstrap_allocators(bootinfo)?;
        Ok(alloc)
    }

    /// Find an untyped of the given size. If one is found, remove
    /// from the list and return it.
    pub fn get_untyped<BitSize: Unsigned>(
        &mut self,
    ) -> Option<LocalCap<Untyped<BitSize, memory_kind::General>>> {
        if let Some(position) = self
            .items
            .iter()
            .position(|ut| ut.size_bits() == BitSize::U8)
        {
            let ut_ref = &self.items[position];
            let ut = Cap {
                cptr: ut_ref.cptr,
                cap_data: PhantomCap::phantom_instance(),
                _role: PhantomData,
            };
            self.items.remove(position);
            return Some(ut);
        }
        None
    }
}

// TODO(dan@auxon.io): I have no idea what to put here.
// N.B.(zack@auxon.io): Linked to another constant with similar need for grounding
const MAX_DEVICE_UTS: usize = MAX_INIT_UNTYPED_ITEMS;

/// An allocator for memory in use by devices.
pub struct DeviceAllocator {
    untypeds: ArrayVec<[LocalCap<WUntyped<memory_kind::Device>>; MAX_DEVICE_UTS]>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum AddressRangeError {
    StartNotPageAligned,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PageAlignedAddressRange {
    start: usize,
    size_bytes: usize,
}

impl PageAlignedAddressRange {
    pub fn new_by_size(
        start: usize,
        size_bytes: usize,
    ) -> Result<PageAlignedAddressRange, AddressRangeError> {
        if start % crate::arch::PageBytes::USIZE != 0 {
            return Err(AddressRangeError::StartNotPageAligned);
        }
        // TODO - add constraints on minimum size and size alignment?
        Ok(PageAlignedAddressRange { start, size_bytes })
    }
}

#[derive(Debug, PartialEq)]
pub enum RangeAllocError {
    AddressStartNotFound,
    AddressFoundButSizeExceedsAvailableMemory,
    RangeSizeNotAPowerOfTwo,
    RangeSizeLessThanMinimumUntypedSize,
    AddressFoundButSizeDoesNotFitInASingleUntyped,
    NotEnoughCNodeSlots,
    SplitError(WUntypedSplitError),
}

impl DeviceAllocator {
    fn get_single_untyped_by_address_range(
        &mut self,
        address_range: PageAlignedAddressRange,
        slots: &mut LocalCap<WCNodeSlotsData<role::Local>>,
    ) -> Result<LocalCap<WUntyped<memory_kind::Device>>, RangeAllocError> {
        let requested_size_bytes = address_range.size_bytes;
        if !requested_size_bytes.is_power_of_two() {
            return Err(RangeAllocError::RangeSizeNotAPowerOfTwo);
        }
        if requested_size_bytes < MIN_UNTYPED_SIZE_BYTES::USIZE {
            return Err(RangeAllocError::RangeSizeLessThanMinimumUntypedSize);
        }
        // This bit calculation assumes that MIN_UNTYPED_SIZE_BYTES >= 0
        // Note that we assume usize >= u32
        let requested_size_bits = requested_size_bytes.trailing_zeros() as usize + 1;

        let mut ut = self
            .get_device_untyped_containing(address_range.start)
            .ok_or_else(|| RangeAllocError::AddressStartNotFound)?;
        let first_found_size = ut.size_bytes();
        if first_found_size == address_range.size_bytes {
            return Ok(ut);
        } else if first_found_size < address_range.size_bytes {
            self.untypeds.push(ut);
            return Err(RangeAllocError::AddressFoundButSizeExceedsAvailableMemory);
        }
        let num_splits = usize::from(ut.cap_data.size_bits) - requested_size_bits;
        if num_splits > slots.size() {
            self.untypeds.push(ut);
            return Err(RangeAllocError::NotEnoughCNodeSlots);
        }
        // Time to do some splitting
        while usize::from(ut.size_bits()) > requested_size_bits {
            let slot_pair = slots
                .alloc_strong::<U2>()
                .map_err(|_| RangeAllocError::NotEnoughCNodeSlots)?;
            let (ut_left, ut_right) = ut
                .split(slot_pair)
                .map_err(|e| RangeAllocError::SplitError(e))?;
            ut = if untyped_contains_paddr(&ut_left, address_range.start) {
                self.untypeds.push(ut_right);
                ut_left
            } else {
                self.untypeds.push(ut_left);
                ut_right
            };

            if address_range.start - ut.paddr() + requested_size_bytes > ut.size_bytes() {
                return Err(RangeAllocError::AddressFoundButSizeDoesNotFitInASingleUntyped);
            }
        }

        if ut.paddr() != address_range.start {
            unreachable!("Split algorithm with assertions on initial address range should always whittle down to the right starting address")
        }
        Ok(ut)
    }
    /// Get the device untyped which contains the given physical
    /// address. If it's present in the list, remove it from the list
    /// and return it.
    fn get_device_untyped_containing(
        &mut self,
        paddr: usize,
    ) -> Option<LocalCap<WUntyped<memory_kind::Device>>> {
        let position = self
            .untypeds
            .iter()
            .position(|ut| untyped_contains_paddr(ut, paddr))?;
        let ut_ref = &self.untypeds[position];
        let ut = Cap {
            cptr: ut_ref.cptr,
            cap_data: WUntyped {
                size_bits: ut_ref.size_bits(),
                kind: ut_ref.cap_data.kind,
            },
            _role: PhantomData,
        };
        self.untypeds.remove(position);
        Some(ut)
    }
}

fn untyped_contains_paddr(ut: &LocalCap<WUntyped<memory_kind::Device>>, paddr: usize) -> bool {
    ut.paddr() <= paddr && ut.paddr() + ut.size_bytes() > paddr
}
