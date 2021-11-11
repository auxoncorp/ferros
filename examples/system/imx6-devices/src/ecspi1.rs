//! ECSPI1

use core::mem;
use core::ops::{Deref, DerefMut};
use static_assertions::const_assert_eq;
use typenum::{Unsigned, U63};

pub type Irq = U63;

register! {
    Rx,
    u32,
    RO,
    Fields [
        Data WIDTH(U32) OFFSET(U0),
    ]
}

register! {
    Tx,
    u32,
    WO,
    Fields [
        Data WIDTH(U32) OFFSET(U0)
    ]
}

register! {
    Control,
    u32,
    RW,
    Fields [
        Enable              WIDTH(U1) OFFSET(U0),
        HardwareTrigger     WIDTH(U1) OFFSET(U1),
        Exchange            WIDTH(U1) OFFSET(U2),
        StartModeControl    WIDTH(U1) OFFSET(U3),
        Channel0Mode        WIDTH(U1) OFFSET(U4) [
            ModeSlave = U0,
            ModeMaster = U1
        ]
        Channel1Mode        WIDTH(U1) OFFSET(U5) [
            ModeSlave = U0,
            ModeMaster = U1
        ]
        Channel2Mode        WIDTH(U1) OFFSET(U6) [
            ModeSlave = U0,
            ModeMaster = U1
        ]
        Channel3Mode        WIDTH(U1) OFFSET(U7) [
            ModeSlave = U0,
            ModeMaster = U1
        ]
        PostDivider         WIDTH(U4) OFFSET(U8),
        PreDivider          WIDTH(U4) OFFSET(U12),
        DataReadyControl    WIDTH(U2) OFFSET(U16) [
            Any = U0,
            EdgeTriggered = U1,
            LevelTriggered = U2
        ]
        ChannelSelect       WIDTH(U2) OFFSET(U18) [
            ChipSelect0 = U0,
            ChipSelect1 = U1,
            ChipSelect2 = U2,
            ChipSelect3 = U3
        ]
        BurstLength         WIDTH(U12) OFFSET(U20)
    ]
}

register! {
    Config,
    u32,
    RW,
    Fields [
        Channel0Phase           WIDTH(U1) OFFSET(U0) [
            Phase0 = U0,
            Phase1 = U1
        ]
        Channel1Phase           WIDTH(U1) OFFSET(U1) [
            Phase0 = U0,
            Phase1 = U1
        ]
        Channel2Phase           WIDTH(U1) OFFSET(U2) [
            Phase0 = U0,
            Phase1 = U1
        ]
        Channel3Phase           WIDTH(U1) OFFSET(U3) [
            Phase0 = U0,
            Phase1 = U1
        ]
        Channel0Polarity        WIDTH(U1) OFFSET(U4) [
            ActiveHigh = U0,
            ActiveLow = U1
        ]
        Channel1Polarity        WIDTH(U1) OFFSET(U5) [
            ActiveHigh = U0,
            ActiveLow = U1
        ]
        Channel2Polarity        WIDTH(U1) OFFSET(U6) [
            ActiveHigh = U0,
            ActiveLow = U1
        ]
        Channel3Polarity        WIDTH(U1) OFFSET(U7) [
            ActiveHigh = U0,
            ActiveLow = U1
        ]
        Channel0WaveFromSelect  WIDTH(U1) OFFSET(U8),
        Channel1WaveFromSelect  WIDTH(U1) OFFSET(U9),
        Channel2WaveFromSelect  WIDTH(U1) OFFSET(U10),
        Channel3WaveFromSelect  WIDTH(U1) OFFSET(U11),
        Channel0SSPolarity      WIDTH(U1) OFFSET(U12) [
            ActiveLow = U0,
            ActiveHigh = U1
        ]
        Channel1SSPolarity      WIDTH(U1) OFFSET(U13) [
            ActiveLow = U0,
            ActiveHigh = U1
        ]
        Channel2SSPolarity      WIDTH(U1) OFFSET(U14) [
            ActiveLow = U0,
            ActiveHigh = U1
        ]
        Channel3SSPolarity      WIDTH(U1) OFFSET(U15) [
            ActiveLow = U0,
            ActiveHigh = U1
        ]
        Channel0DataCtl        WIDTH(U1) OFFSET(U16) [
            StayHigh = U0,
            StayLow = U1
        ]
        Channel1DataCtl        WIDTH(U1) OFFSET(U17) [
            StayHigh = U0,
            StayLow = U1
        ]
        Channel2DataCtl        WIDTH(U1) OFFSET(U18) [
            StayHigh = U0,
            StayLow = U1
        ]
        Channel3DataCtl        WIDTH(U1) OFFSET(U19) [
            StayHigh = U0,
            StayLow = U1
        ]
        Channel0SclkCtl         WIDTH(U1) OFFSET(U20) [
            StayLow = U0,
            StayHigh = U1
        ]
        Channel1SclkCtl         WIDTH(U1) OFFSET(U21) [
            StayLow = U0,
            StayHigh = U1
        ]
        Channel2SclkCtl         WIDTH(U1) OFFSET(U22) [
            StayLow = U0,
            StayHigh = U1
        ]
        Channel3SclkCtl         WIDTH(U1) OFFSET(U23) [
            StayLow = U0,
            StayHigh = U1
        ]
        HtLength  WIDTH(U5) OFFSET(U24)
    ]
}

register! {
    Interrupt,
    u32,
    RW,
    Fields [
        Bits  WIDTH(U8) OFFSET(U0)
    ]
}

register! {
    Status,
    u32,
    RW,
    Fields [
        TxFifoEmpty  WIDTH(U1) OFFSET(U0),
        TxFifoDataReq  WIDTH(U1) OFFSET(U1),
        TxFifoFull  WIDTH(U1) OFFSET(U2),
        RxFifoReady  WIDTH(U1) OFFSET(U3),
        RxFifoDataReq  WIDTH(U1) OFFSET(U4),
        RxFifoFull  WIDTH(U1) OFFSET(U5),
        RxFifoOverflow  WIDTH(U1) OFFSET(U6),
        TransferComplete  WIDTH(U1) OFFSET(U7),
    ]
}

register! {
    Period,
    u32,
    RW,
    Fields [
        SamplePeriod  WIDTH(U14) OFFSET(U0),
        ClockSource  WIDTH(U1) OFFSET(U15) [
            SpiClock = U0,
            RefClock = U1
        ]
    ]
}

const_assert_eq!(mem::size_of::<RegisterBlock>(), 0x20);

#[repr(C)]
pub struct RegisterBlock {
    pub rx: Rx::Register,         // 0x00
    pub tx: Tx::Register,         // 0x04
    pub ctl: Control::Register,   // 0x08
    pub cfg: Config::Register,    // 0x0C
    pub int: Interrupt::Register, // 0x10
    __reserved_0: u32,            // 0x14
    pub status: Status::Register, // 0x18
    pub period: Period::Register, // 0x1C
}

pub struct ECSPI1 {
    vaddr: u32,
}

impl ECSPI1 {
    pub const PADDR: u32 = 0x0200_8000;
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

impl Deref for ECSPI1 {
    type Target = RegisterBlock;
    fn deref(&self) -> &RegisterBlock {
        unsafe { &*self.as_ptr() }
    }
}

impl DerefMut for ECSPI1 {
    fn deref_mut(&mut self) -> &mut RegisterBlock {
        unsafe { &mut *self.as_mut_ptr() }
    }
}
