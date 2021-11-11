//! GPIO

use core::mem;
use core::ops::{Deref, DerefMut};
use static_assertions::const_assert_eq;
use typenum::Unsigned;

register! {
    Data,
    u32,
    RW,
    Fields [
        Bits  WIDTH(U32) OFFSET(U0),
    ]
}

register! {
    Direction,
    u32,
    RW,
    Fields [
        Bits  WIDTH(U32) OFFSET(U0),
    ]
}

register! {
    Status,
    u32,
    RO,
    Fields [
        Bits  WIDTH(U32) OFFSET(U0),
    ]
}

register! {
    IntConfig1,
    u32,
    RW,
    Fields [
        Bits  WIDTH(U32) OFFSET(U0),
    ]
}

register! {
    IntConfig2,
    u32,
    RW,
    Fields [
        Bits  WIDTH(U32) OFFSET(U0),
    ]
}

register! {
    IntMask,
    u32,
    RW,
    Fields [
        Bits  WIDTH(U32) OFFSET(U0),
    ]
}

register! {
    IntStatus,
    u32,
    RW,
    Fields [
        Bits  WIDTH(U32) OFFSET(U0),
    ]
}

register! {
    EdgeSelect,
    u32,
    RW,
    Fields [
        Bits  WIDTH(U32) OFFSET(U0),
    ]
}

const_assert_eq!(mem::size_of::<RegisterBlock>(), 0x20);

#[repr(C)]
pub struct RegisterBlock {
    pub data: Data::Register,              // 0x00
    pub direction: Direction::Register,    // 0x04
    pub status: Status::Register,          // 0x08
    pub int_cfg1: IntConfig1::Register,    // 0x0C
    pub int_cfg2: IntConfig2::Register,    // 0x10
    pub int_mask: IntMask::Register,       // 0x14
    pub int_status: IntStatus::Register,   // 0x18
    pub edge_select: EdgeSelect::Register, // 0x1C
}

pub const NUM_BLOCKS: usize = 7;

macro_rules! gpio_pins {
    ($GPIOx:ident, $PADDR:literal) => {
        pub struct $GPIOx {
            vaddr: u32,
        }

        impl $GPIOx {
            pub const PADDR: u32 = $PADDR;
            pub const SIZE: usize = crate::PageBytes::USIZE;

            /// # Safety
            /// out of thin air
            pub unsafe fn from_vaddr(vaddr: u32) -> Self {
                Self { vaddr }
            }

            fn as_ptr(&self) -> *const RegisterBlock {
                self.vaddr as *const _
            }

            fn as_mut_ptr(&mut self) -> *mut RegisterBlock {
                self.vaddr as *mut _
            }
        }

        impl Deref for $GPIOx {
            type Target = RegisterBlock;
            fn deref(&self) -> &RegisterBlock {
                unsafe { &*self.as_ptr() }
            }
        }

        impl DerefMut for $GPIOx {
            fn deref_mut(&mut self) -> &mut RegisterBlock {
                unsafe { &mut *self.as_mut_ptr() }
            }
        }
    };
}

gpio_pins!(GPIO1, 0x0209_C000);
gpio_pins!(GPIO2, 0x020A_0000);
gpio_pins!(GPIO3, 0x020A_4000);
gpio_pins!(GPIO4, 0x020A_8000);
gpio_pins!(GPIO5, 0x020A_C000);
gpio_pins!(GPIO6, 0x020B_0000);
gpio_pins!(GPIO7, 0x020B_4000);
