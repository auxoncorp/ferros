//! GPT

use core::mem;
use core::ops::{Deref, DerefMut};
use static_assertions::const_assert_eq;
use typenum::{Unsigned, U87};

pub type Irq = U87;

register! {
    Control,
    u32,
    RW,
    Fields [
        Enable              WIDTH(U1) OFFSET(U0),
        EnableMode          WIDTH(U1) OFFSET(U1),
        DebugMode           WIDTH(U1) OFFSET(U2),
        WaitMode            WIDTH(U1) OFFSET(U3),
        DozeMode            WIDTH(U1) OFFSET(U4),
        StopMode            WIDTH(U1) OFFSET(U5),
        ClockSource         WIDTH(U3) OFFSET(U6) [
            NoClock = U0,
            PeripheralClock = U1,
            CrystalOscDiv8 = U5,
            CrystalOsc = U7
        ]
        FreeRunRestartMode  WIDTH(U1) OFFSET(U9) [
            RestartMode = U0,
            FreeRunMode = U1
        ]
        Enable24MClock      WIDTH(U1) OFFSET(U10),
        SwReset             WIDTH(U1) OFFSET(U15),
    ]
}

register! {
    Prescale,
    u32,
    RW,
    Fields [
        Prescaler       WIDTH(U12) OFFSET(U0) [
            Div1 = U0
        ]
        Prescaler24M    WIDTH(U14) OFFSET(U12) [
            Div1 = U0
        ]
    ]
}

register! {
    Status,
    u32,
    RW,
    Fields [
        OutputCompare1  WIDTH(U1) OFFSET(U0),
        OutputCompare2  WIDTH(U1) OFFSET(U1),
        OutputCompare3  WIDTH(U1) OFFSET(U2),
        InputCapture1   WIDTH(U1) OFFSET(U3),
        InputCapture2   WIDTH(U1) OFFSET(U4),
        RollOver        WIDTH(U1) OFFSET(U5),
    ]
}

register! {
    Interrupt,
    u32,
    RW,
    Fields [
        OutputCompare1  WIDTH(U1) OFFSET(U0),
        OutputCompare2  WIDTH(U1) OFFSET(U1),
        OutputCompare3  WIDTH(U1) OFFSET(U2),
        InputCapture1   WIDTH(U1) OFFSET(U3),
        InputCapture2   WIDTH(U1) OFFSET(U4),
        RollOver        WIDTH(U1) OFFSET(U5),
    ]
}

register! {
    OutputCompare1,
    u32,
    RW,
    Fields [
        Compare         WIDTH(U32) OFFSET(U0),
    ]
}

register! {
    OutputCompare2,
    u32,
    RW,
    Fields [
        Compare         WIDTH(U32) OFFSET(U0),
    ]
}

register! {
    OutputCompare3,
    u32,
    RW,
    Fields [
        Compare         WIDTH(U32) OFFSET(U0),
    ]
}

register! {
    InputCapture1,
    u32,
    RO,
    Fields [
        Caputre         WIDTH(U32) OFFSET(U0),
    ]
}

register! {
    InputCapture2,
    u32,
    RO,
    Fields [
        Caputre         WIDTH(U32) OFFSET(U0),
    ]
}

register! {
    Counter,
    u32,
    RO,
    Fields [
        Count           WIDTH(U32) OFFSET(U0),
    ]
}

const_assert_eq!(mem::size_of::<RegisterBlock>(), 0x28);

#[repr(C)]
pub struct RegisterBlock {
    pub cr: Control::Register,          // 0x00
    pub pr: Prescale::Register,         // 0x04
    pub sr: Status::Register,           // 0x08
    pub ir: Interrupt::Register,        // 0x0C
    pub ocr1: OutputCompare1::Register, // 0x10
    pub ocr2: OutputCompare2::Register, // 0x14
    pub ocr3: OutputCompare3::Register, // 0x18
    pub icr1: InputCapture1::Register,  // 0x1C
    pub icr2: InputCapture2::Register,  // 0x20
    pub cnt: Counter::Register,         // 0x24
}

pub struct GPT {
    vaddr: u32,
}

impl GPT {
    pub const PADDR: u32 = 0x0209_8000;
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

impl Deref for GPT {
    type Target = RegisterBlock;
    fn deref(&self) -> &RegisterBlock {
        unsafe { &*self.as_ptr() }
    }
}

impl DerefMut for GPT {
    fn deref_mut(&mut self) -> &mut RegisterBlock {
        unsafe { &mut *self.as_mut_ptr() }
    }
}
