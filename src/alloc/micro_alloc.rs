//! A tiny first-chance allocator for the untyped capabilities sel4's BOOTINFO.
//! This one doesn't split anything; it just hands out the smallest untyped item
//! that's big enough for the request.
use core::fmt::{Debug, Error as FmtError, Formatter};
use core::marker::PhantomData;

use selfe_sys::seL4_BootInfo;

use crate::arch::MaxNaiveSplitCount;
use crate::arch::MaxUntypedSize as MaxUntypedSizeBits;
use crate::arch::MinUntypedSize as MinUntypedSizeBits;
use crate::cap::{
    memory_kind, role, Cap, LocalCNodeSlots, LocalCap, PhantomCap, Untyped, WCNodeSlotsData,
    WUntyped, WUntypedSplitError,
};
use crate::pow::Pow;
use arrayvec::ArrayVec;
use core::convert::{TryFrom, TryInto};
use typenum::*;

pub type MinUntypedSizeBytes = Pow<MinUntypedSizeBits>;
pub type MaxUntypedSizeBytes = Pow<MaxUntypedSizeBits>;

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
    let mut device_uts: ArrayVec<[LocalCap<WUntyped<memory_kind::Device>>; MAX_DEVICE_UTS]> =
        ArrayVec::new();

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
    // N.B. could cut the pdqsort dependency by doing this sorting during the
    // initial insertion
    pdqsort::sort_by_key(&mut device_uts, |wut| wut.cap_data.kind.paddr);
    Ok((
        Allocator { items: general_uts },
        DeviceAllocator {
            untypeds: device_uts,
        },
    ))
}

/// An allocator for general purpose memory.
pub struct Allocator {
    pub(super) items: ArrayVec<[LocalCap<WUntyped<memory_kind::General>>; MAX_INIT_UNTYPED_ITEMS]>,
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
        let position = self
            .items
            .iter()
            .position(|ut| ut.size_bits() == BitSize::U8)?;
        let ut_ref = &self.items[position];
        let ut = Cap {
            cptr: ut_ref.cptr,
            cap_data: PhantomCap::phantom_instance(),
            _role: PhantomData,
        };
        self.items.remove(position);
        Some(ut)
    }
}

// TODO(dan@auxon.io): I have no idea what to put here.
// N.B.(zack@auxon.io): Linked to another constant with similar need for
// grounding
const MAX_DEVICE_UTS: usize = MAX_INIT_UNTYPED_ITEMS;

/// An allocator for memory in use by devices.
pub struct DeviceAllocator {
    untypeds: ArrayVec<[LocalCap<WUntyped<memory_kind::Device>>; MAX_DEVICE_UTS]>,
}

impl Debug for DeviceAllocator {
    fn fmt(&self, f: &mut Formatter) -> Result<(), FmtError> {
        f.write_str("DeviceAllocator {")?;
        write!(f, "\n  num_untypeds: {}", self.untypeds.len())?;
        f.write_str("\n  untypeds: [")?;
        for i in &self.untypeds {
            let paddr = i.cap_data.kind.paddr;
            write!(
                f,
                "\n    {{ cptr: {}, size_bits: {}, paddr: {:#018X?}, end_paddr: {:#018X?} }},",
                i.cptr,
                i.size_bits(),
                paddr,
                paddr + 2usize.pow(u32::from(i.size_bits()))
            )
            .unwrap();
        }
        f.write_str("\n  ]")?;
        f.write_str("\n}")
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum PageAlignedAddressRangeError {
    StartNotPageAligned,
    SizeNotPageAligned,
    SizeLessThanAPage,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PageAlignedAddressRange {
    start: PageAligned,
    /// Must be at least one page in size
    size_bytes: PageAligned,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PageAligned(pub usize);

impl TryFrom<usize> for PageAligned {
    type Error = NotPageAligned;

    fn try_from(value: usize) -> Result<PageAligned, Self::Error> {
        if value % crate::arch::PageBytes::USIZE == 0 {
            Ok(PageAligned(value))
        } else {
            Err(NotPageAligned)
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub struct NotPageAligned;

impl PageAlignedAddressRange {
    pub fn new_by_size(
        start: usize,
        size_bytes: usize,
    ) -> Result<PageAlignedAddressRange, PageAlignedAddressRangeError> {
        let start = start
            .try_into()
            .map_err(|_| PageAlignedAddressRangeError::StartNotPageAligned)?;
        if size_bytes < crate::arch::PageBytes::USIZE {
            return Err(PageAlignedAddressRangeError::SizeLessThanAPage);
        }
        let size_bytes = size_bytes
            .try_into()
            .map_err(|_| PageAlignedAddressRangeError::SizeNotPageAligned)?;
        Ok(PageAlignedAddressRange { start, size_bytes })
    }
}

#[derive(Debug, PartialEq)]
pub enum DeviceRangeAllocError {
    // Not enough internal storage space
    TooManyDeviceUntypeds,
    // Error variants that might be moved over to the range type
    RangeSizeNotAPowerOfTwo,
    RangeSizeLessThanMinimumUntypedSize,
    RangeSizeGreaterThanMaximumUntypedSize,

    // Error variants limited by what's available in the allocator state
    AddressStartNotFound,
    AddressFoundButSizeExceedsAvailableMemory,
    AddressFoundButSizeDoesNotFitInASingleUntyped,
    AddressStartInTheMiddleOfTargetSizeUntyped,

    SplitError(WUntypedSplitError),

    // Only relevant when we don't go out of our way to provide excessive slots.
    NotEnoughCNodeSlots,
}

impl DeviceAllocator {
    pub fn get_untyped_by_address_range_slot_infallible(
        &mut self,
        address_range: PageAlignedAddressRange,
        slots: LocalCNodeSlots<op!(MaxNaiveSplitCount + MaxNaiveSplitCount)>,
    ) -> Result<LocalCap<WUntyped<memory_kind::Device>>, DeviceRangeAllocError> {
        let mut slots = slots.weaken();
        self.get_untyped_by_address_range(address_range, &mut slots)
            .map_err(|e| match e {
                DeviceRangeAllocError::NotEnoughCNodeSlots => {
                    unreachable!("Should be logically impossible to run out of slots")
                }
                _ => e,
            })
    }

    /// Extract a single device-memory-backed untyped based on its starting
    /// address and size. If the requested memory range is managed by this
    /// allocator, but does not already exist as an Untyped region of the
    /// desired size, the allocator will split apart the larger
    /// memory chunks containing the region of interest just enough to get out
    /// exactly the requested range, consuming CNode slots as it does so.
    pub fn get_untyped_by_address_range(
        &mut self,
        address_range: PageAlignedAddressRange,
        slots: &mut LocalCap<WCNodeSlotsData<role::Local>>,
    ) -> Result<LocalCap<WUntyped<memory_kind::Device>>, DeviceRangeAllocError> {
        let requested_size_bytes = address_range.size_bytes.0;
        if requested_size_bytes >= MaxUntypedSizeBytes::USIZE {
            return Err(DeviceRangeAllocError::RangeSizeGreaterThanMaximumUntypedSize);
        }
        if requested_size_bytes < crate::arch::PageBytes::USIZE {
            return Err(DeviceRangeAllocError::RangeSizeLessThanMinimumUntypedSize);
        }
        if !requested_size_bytes.is_power_of_two() {
            return Err(DeviceRangeAllocError::RangeSizeNotAPowerOfTwo);
        }
        // This bit calculation assumes that PageBytes > 0
        // and the above is_power_of_two check
        // Note that we assume usize >= u32
        let requested_size_bits = requested_size_bytes.trailing_zeros() as usize;

        let mut ut = self
            .get_device_untyped_containing(address_range.start.0)
            .ok_or(DeviceRangeAllocError::AddressStartNotFound)?;
        let first_found_size = ut.size_bytes();

        if first_found_size < requested_size_bytes {
            self.insert_sorted(ut)
                .map_err(|_| DeviceRangeAllocError::TooManyDeviceUntypeds)?;
            return Err(DeviceRangeAllocError::AddressFoundButSizeExceedsAvailableMemory);
        }
        if first_found_size == requested_size_bytes {
            if ut.paddr() == address_range.start.0 {
                return Ok(ut);
            } else {
                self.insert_sorted(ut)
                    .map_err(|_| DeviceRangeAllocError::TooManyDeviceUntypeds)?;
                return Err(DeviceRangeAllocError::AddressStartInTheMiddleOfTargetSizeUntyped);
            }
        }

        let num_splits = usize::from(ut.cap_data.size_bits) - requested_size_bits;
        if 2 * num_splits > slots.size() {
            self.insert_sorted(ut)
                .map_err(|_| DeviceRangeAllocError::TooManyDeviceUntypeds)?;
            return Err(DeviceRangeAllocError::NotEnoughCNodeSlots);
        }

        // Time to do some splitting
        // Note that we take care to maintain the "all untypeds are sorted by paddr"
        // invariant at every early exit point, while only doing a single
        // full-sort per call of this method
        while usize::from(ut.size_bits()) > requested_size_bits {
            let slot_pair = slots.alloc_strong::<U2>().map_err(|_| {
                pdqsort::sort_by_key(&mut self.untypeds, |wut| wut.cap_data.kind.paddr);
                DeviceRangeAllocError::NotEnoughCNodeSlots
            })?;
            let (ut_left, ut_right) = ut.split(slot_pair).map_err(|e| {
                pdqsort::sort_by_key(&mut self.untypeds, |wut| wut.cap_data.kind.paddr);
                DeviceRangeAllocError::SplitError(e)
            })?;
            ut = if untyped_contains_paddr(&ut_left, address_range.start.0) {
                self.untypeds.try_push(ut_right).map_err(|_| {
                    pdqsort::sort_by_key(&mut self.untypeds, |wut| wut.cap_data.kind.paddr);
                    DeviceRangeAllocError::TooManyDeviceUntypeds
                })?;
                ut_left
            } else {
                self.untypeds.try_push(ut_left).map_err(|_| {
                    pdqsort::sort_by_key(&mut self.untypeds, |wut| wut.cap_data.kind.paddr);
                    DeviceRangeAllocError::TooManyDeviceUntypeds
                })?;
                ut_right
            };

            if address_range.start.0 - ut.paddr() + requested_size_bytes > ut.size_bytes() {
                pdqsort::sort_by_key(&mut self.untypeds, |wut| wut.cap_data.kind.paddr);
                return Err(DeviceRangeAllocError::AddressFoundButSizeDoesNotFitInASingleUntyped);
            }
        }

        // Finally sort all of those untypeds that were pushed during splitting
        pdqsort::sort_by_key(&mut self.untypeds, |wut| wut.cap_data.kind.paddr);

        if ut.paddr() != address_range.start.0 {
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

    fn insert_sorted(
        &mut self,
        fresh: LocalCap<WUntyped<memory_kind::Device>>,
    ) -> Result<(), arrayvec::CapacityError<LocalCap<WUntyped<memory_kind::Device>>>> {
        let paddr = fresh.cap_data.kind.paddr;
        if let Some(pos) = self
            .untypeds
            .iter()
            .enumerate()
            .find(|(_pos, wut)| wut.cap_data.kind.paddr > paddr)
            .map(|(pos, _wut)| pos)
        {
            self.untypeds.try_insert(pos, fresh)
        } else {
            self.untypeds.try_push(fresh)
        }
    }
}

fn untyped_contains_paddr(ut: &LocalCap<WUntyped<memory_kind::Device>>, paddr: usize) -> bool {
    ut.paddr() <= paddr && ut.paddr() + ut.size_bytes() > paddr
}
