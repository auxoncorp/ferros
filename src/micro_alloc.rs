//! A tiny first-chance allocator for the untyped capabilities sel4's BOOTINFO.
//! This one doesn't split anything; it just hands out the smallest untyped item
//! that's big enough for the request.

use crate::fancy::{wrap_untyped, Capability, Untyped};
use arrayvec::ArrayVec;
use typenum::Unsigned;

use sel4_sys::{seL4_BootInfo, seL4_UntypedDesc};

pub const MIN_UNTYPED_SIZE: u8 = 4;
pub const MAX_UNTYPED_SIZE: u8 = 32;

// TODO - pull from configs
pub const MAX_INIT_UNTYPED_ITEMS: usize = 256;

struct UntypedItem {
    cptr: usize,
    desc: &'static seL4_UntypedDesc,
    is_free: bool,
}

#[derive(Debug)]
pub enum Error {
    InvalidBootInfoCapability,
    UntypedSizeOutOfRange,
}

impl UntypedItem {
    pub fn new(cptr: usize, desc: &'static seL4_UntypedDesc) -> Result<UntypedItem, Error> {
        if cptr == 0 {
            Err(Error::InvalidBootInfoCapability)
        } else if desc.sizeBits < MIN_UNTYPED_SIZE || desc.sizeBits > MAX_UNTYPED_SIZE {
            Err(Error::UntypedSizeOutOfRange)
        } else {
            Ok(UntypedItem {
                cptr,
                desc,
                is_free: true,
            })
        }
    }
}

pub struct Allocator {
    items: ArrayVec<[UntypedItem; MAX_INIT_UNTYPED_ITEMS]>,
}

impl Allocator {
    pub fn bootstrap(bootinfo: &'static seL4_BootInfo) -> Result<Allocator, Error> {
        let untyped_item_iter = (0..(bootinfo.untyped.end - bootinfo.untyped.start)).map(|i| {
            UntypedItem::new(
                (bootinfo.untyped.start + i) as usize,              // cptr
                &bootinfo.untypedList[i as usize],
            )
        });

        if let Some(Some(e)) = untyped_item_iter
            .clone()
            .map(|i| match i {
                Ok(_) => None,
                Err(e) => Some(e),
            })
            .find(|o| o.is_some())
        {
            return Err(e);
        }

        Ok(Allocator {
            items: untyped_item_iter.map(|i| i.unwrap()).collect(),
        })
    }
}

pub trait GetUntyped {
    fn get_untyped<BitSize: Unsigned>(&mut self) -> Option<Capability<Untyped<BitSize>>>;
}

impl GetUntyped for Allocator {
    fn get_untyped<BitSize: Unsigned>(&mut self) -> Option<Capability<Untyped<BitSize>>> {
        // This is very inefficient. But it should only be called a small
        // handful of times on startup.
        for bit_size in BitSize::to_u8()..=MAX_UNTYPED_SIZE {
            for item in &mut self.items {
                if item.desc.sizeBits == bit_size {
                    return wrap_untyped(item.cptr, item.desc);
                }
            }
        }

        None
    }
}
