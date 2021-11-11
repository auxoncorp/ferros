//! UART1
//! See [IMX6DQRM](http://cache.freescale.com/files/32bit/doc/ref_manual/IMX6DQRM.pdf) chapter 64.

use core::mem;
use core::ops::{Deref, DerefMut};
use static_assertions::const_assert_eq;
use typenum::{Unsigned, U58};

pub type Irq = U58;

register! {
    Rx,
    u32,
    RO,
    Fields [
        Data        WIDTH(U8) OFFSET(U0),
        ParityError WIDTH(U1) OFFSET(U10),
        Brk         WIDTH(U1) OFFSET(U11),
        FrameError  WIDTH(U1) OFFSET(U12),
        Overrrun    WIDTH(U1) OFFSET(U13),
        Error       WIDTH(U1) OFFSET(U14),
        ChrRdy      WIDTH(U1) OFFSET(U15)
    ]
}

register! {
    Tx,
    u32,
    WO,
    Fields [
        Data WIDTH(U8) OFFSET(U0)
    ]
}

register! {
    Control1,
    u32,
    RW,
    Fields [
        Enable              WIDTH(U1) OFFSET(U0),
        Doze                WIDTH(U1) OFFSET(U1),
        AgingDMATimerEnable WIDTH(U1) OFFSET(U2),
        TxRdyDMAENable      WIDTH(U1) OFFSET(U3),
        SendBreak           WIDTH(U1) OFFSET(U4),
        RTSDeltaInterrupt   WIDTH(U1) OFFSET(U5),
        TxEmptyInterrupt    WIDTH(U1) OFFSET(U6),
        Infrared            WIDTH(U1) OFFSET(U7),
        RecvReadyDMA        WIDTH(U1) OFFSET(U8),
        RecvReadyInterrupt  WIDTH(U1) OFFSET(U9),
        IdleCondition       WIDTH(U2) OFFSET(U10),
        IdleInterrupt       WIDTH(U1) OFFSET(U12),
        TxReadyInterrupt    WIDTH(U1) OFFSET(U13),
        AutoBaud            WIDTH(U1) OFFSET(U14),
        AutoBaudInterrupt   WIDTH(U1) OFFSET(U15)
    ]
}

register! {
    Control2,
    u32,
    RW,
    Fields [
        SoftwareReset      WIDTH(U1) OFFSET(U0),
        RxEnable           WIDTH(U1) OFFSET(U1),
        TxEnable           WIDTH(U1) OFFSET(U2),
        AgingTimer         WIDTH(U1) OFFSET(U3),
        ReqSendInterrupt   WIDTH(U1) OFFSET(U4),
        WordSize           WIDTH(U1) OFFSET(U5),
        TwoStopBits        WIDTH(U1) OFFSET(U6),
        ParityOddEven      WIDTH(U1) OFFSET(U7),
        ParityEnable       WIDTH(U1) OFFSET(U8),
        RequestToSendEdge  WIDTH(U2) OFFSET(U9),
        Escape             WIDTH(U1) OFFSET(U11),
        ClearToSend        WIDTH(U1) OFFSET(U12),
        ClearToSendControl WIDTH(U1) OFFSET(U13),
        IgnoreRTS          WIDTH(U1) OFFSET(U14),
        EscapeInterrupt    WIDTH(U1) OFFSET(U15)
    ]
}

register! {
    Status2,
    u32,
    RW,
    Fields [
        RxDataReady  WIDTH(U1) OFFSET(U0),
        TxFifoEmpty  WIDTH(U1) OFFSET(U14)
    ]
}

const_assert_eq!(mem::size_of::<RegisterBlock>(), 0x9C);

#[repr(C)]
pub struct RegisterBlock {
    pub rx: Rx::Register,         // 0x00
    __reserved_0: [u32; 15],      // 0x04
    pub tx: Tx::Register,         // 0x40
    __reserved_1: [u32; 15],      // 0x44
    pub ctl1: Control1::Register, // 0x80
    pub ctl2: Control2::Register, // 0x84
    __reserved_2: [u32; 4],       // 0x88
    pub stat2: Status2::Register, // 0x98
}

pub struct UART1 {
    vaddr: u32,
}

impl UART1 {
    pub const PADDR: u32 = 0x0202_0000;
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

impl Deref for UART1 {
    type Target = RegisterBlock;
    fn deref(&self) -> &RegisterBlock {
        unsafe { &*self.as_ptr() }
    }
}

impl DerefMut for UART1 {
    fn deref_mut(&mut self) -> &mut RegisterBlock {
        unsafe { &mut *self.as_mut_ptr() }
    }
}
