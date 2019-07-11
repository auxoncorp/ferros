//! A tiny first-chance allocator for the untyped capabilities sel4's BOOTINFO.
//! This one doesn't split anything; it just hands out the smallest untyped item
//! that's big enough for the request.
use core::fmt::{Debug, Error as FmtError, Formatter};
use core::marker::PhantomData;

use selfe_sys::seL4_BootInfo;

use crate::cap::{memory_kind, Cap, LocalCap, PhantomCap, Untyped, WUntyped};
use arrayvec::ArrayVec;
use typenum::Unsigned;

pub const MIN_UNTYPED_SIZE_BITS: u8 = 4;
pub const MAX_UNTYPED_SIZE_BITS: u8 = 32;

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
                    size_bits: ut.sizeBits as usize,
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
                    size_bits: ut.sizeBits as usize,
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
            .position(|ut| ut.size_bits() == BitSize::USIZE)
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

impl DeviceAllocator {
    /// Get the device untyped which contains the given physical
    /// address. If it's present in the list, remove it from the list
    /// and return it.
    pub fn get_device_untyped(
        &mut self,
        paddr: usize,
    ) -> Option<LocalCap<WUntyped<memory_kind::Device>>> {
        let untyped_contains_paddr = |ut: &LocalCap<WUntyped<memory_kind::Device>>| -> bool {
            ut.paddr() <= paddr && ut.paddr() + ut.size_bytes() > paddr
        };

        if let Some(position) = self
            .untypeds
            .iter()
            .position(|ut| untyped_contains_paddr(ut))
        {
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
            return Some(ut);
        }
        None
    }
}
