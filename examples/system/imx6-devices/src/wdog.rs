//! WDOG
//! See [IMX6DQRM](http://cache.freescale.com/files/32bit/doc/ref_manual/IMX6DQRM.pdf) chapter 70.

use core::mem;
use core::ops::{Deref, DerefMut};
use static_assertions::const_assert_eq;
use typenum::{Unsigned, U112, U113};

pub const SEQUENCE_A: u16 = 0x5555;
pub const SEQUENCE_B: u16 = 0xAAAA;

register! {
    Control,
    u16,
    RW,
    Fields [
        LowPower            WIDTH(U1) OFFSET(U0) [
            Continue = U0,
            Suspend = U1
        ]
        Debug               WIDTH(U1) OFFSET(U1)[
            Continue = U0,
            Suspend = U1
        ]
        Enable              WIDTH(U1) OFFSET(U2),
        AssertOnTimeout     WIDTH(U1) OFFSET(U3),
        SwResetSignal       WIDTH(U1) OFFSET(U4) [
            AssertReset = U0,
            NoEffect = U1
        ]
        Assert              WIDTH(U1) OFFSET(U5),
        DisableForWait      WIDTH(U1) OFFSET(U7),
        Timeout             WIDTH(U8) OFFSET(U8),
    ]
}

register! {
    Service,
    u16,
    RW,
    Fields [
        Sequence WIDTH(U16) OFFSET(U0),
    ]
}

register! {
    ResetStatus,
    u16,
    RO,
    Fields [
        SwReset             WIDTH(U1) OFFSET(U0),
        Timeout             WIDTH(U1) OFFSET(U1),
        PowerOnReset        WIDTH(U1) OFFSET(U4),
    ]
}

register! {
    InterruptControl,
    u16,
    RW,
    Fields [
        CounterTimeout     WIDTH(U8) OFFSET(U0),
        InterruptStatus    WIDTH(U1) OFFSET(U14),
        InterruptEnable    WIDTH(U1) OFFSET(U15),
    ]
}

register! {
    MiscControl,
    u16,
    RW,
    Fields [
        PowerDownEnable  WIDTH(U1) OFFSET(U0),
    ]
}

const_assert_eq!(mem::size_of::<RegisterBlock>(), 0x0A);

// NOTE: these are 16-bit registers
#[repr(C)]
pub struct RegisterBlock {
    pub wcr: Control::Register,           // 0x00
    pub wsr: Service::Register,           // 0x02
    pub wrsr: ResetStatus::Register,      // 0x04
    pub wicr: InterruptControl::Register, // 0x06
    pub wmcr: MiscControl::Register,      // 0x08
}

pub mod wdog1 {
    use super::*;

    pub type Irq = U112;

    pub struct WDOG1 {
        vaddr: u32,
    }

    impl WDOG1 {
        pub const PADDR: u32 = 0x020B_C000;
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

    impl Deref for WDOG1 {
        type Target = RegisterBlock;
        fn deref(&self) -> &RegisterBlock {
            unsafe { &*self.as_ptr() }
        }
    }

    impl DerefMut for WDOG1 {
        fn deref_mut(&mut self) -> &mut RegisterBlock {
            unsafe { &mut *self.as_mut_ptr() }
        }
    }
}

pub mod wdog2 {
    use super::*;

    pub type Irq = U113;

    pub struct WDOG2 {
        vaddr: u32,
    }

    impl WDOG2 {
        pub const PADDR: u32 = 0x020C_0000;
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

    impl Deref for WDOG2 {
        type Target = RegisterBlock;
        fn deref(&self) -> &RegisterBlock {
            unsafe { &*self.as_ptr() }
        }
    }

    impl DerefMut for WDOG2 {
        fn deref_mut(&mut self) -> &mut RegisterBlock {
            unsafe { &mut *self.as_mut_ptr() }
        }
    }
}
