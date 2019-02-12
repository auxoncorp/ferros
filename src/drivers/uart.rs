use typenum::consts::U256;
use typenum::{IsLess, True, Unsigned};

use registers::{ReadOnlyRegister, WriteOnlyRegister};

use crate::userland::{role, CNodeRole, InterruptConsumer, RetypeForSetup};

pub struct UartParams<IRQ: Unsigned + Sync + Send, Role: CNodeRole>
where
    IRQ: IsLess<U256, Output = True>,
{
    pub base_ptr: usize,
    pub consumer: InterruptConsumer<IRQ, Role>,
}

impl<IRQ: Unsigned + Sync + Send> RetypeForSetup for UartParams<IRQ, role::Local>
where
    IRQ: IsLess<U256, Output = True>,
{
    type Output = UartParams<IRQ, role::Child>;
}

//todo: Get the status registers here too.

register! {
    UartRX,
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

pub const TX_OFFSET: usize = 1 << 6;

register! {
    UartTX,
    WO,
    Fields [
        Data WIDTH(U8) OFFSET(U0)
    ]
}

pub const CTL1_OFFSET: usize = 1 << 7;

register! {
    UartControl1,
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
    UartControl2,
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
    UartControl3,
    RW,
    Fields [
        AutoBaudCounterInterrupt   WIDTH(U1) OFFSET(U0),
        InvertTX                   WIDTH(U1) OFFSET(U1),
        RXDMuxed                   WIDTH(U1) OFFSET(U2),
        DataTermReadyDelta         WIDTH(U1) OFFSET(U3),
        AsyncWakeInterrupt         WIDTH(U1) OFFSET(U4),
        AsyncIRInterrupt           WIDTH(U1) OFFSET(U5),
        RxStatusInterrupt          WIDTH(U1) OFFSET(U6),
        AutoBaudNotImproved        WIDTH(U1) OFFSET(U7),
        RingIndicator              WIDTH(U1) OFFSET(U8),
        DataCarrierDetect          WIDTH(U1) OFFSET(U9),
        DatSetReady                WIDTH(U1) OFFSET(U10),
        FrameErrorInterrupt        WIDTH(U1) OFFSET(U11),
        ParityErrorInterrupt       WIDTH(U1) OFFSET(U12),
        DataTerminalReadyInterrupt WIDTH(U1) OFFSET(U13),
        DTRInterruptEnable         WIDTH(U2) OFFSET(U14)
    ]
}

register! {
    UartControl4,
    RW,
    Fields [
        RxDataReadyInterrupt WIDTH(U1) OFFSET(U0),
        RxOverrunInterrupt   WIDTH(U1) OFFSET(U1),
        BreakCondInterrupt   WIDTH(U1) OFFSET(U2),
        TxCompleteInterrupt  WIDTH(U1) OFFSET(U3),
        LowPowerBypass       WIDTH(U1) OFFSET(U4),
        IRSpecialCase        WIDTH(U1) OFFSET(U5),
        DMAIdleCondInterrupt WIDTH(U1) OFFSET(U6),
        WakeInterrupt        WIDTH(U1) OFFSET(U7),
        SerialIRInterrupt    WIDTH(U1) OFFSET(U8),
        InvertRX             WIDTH(U1) OFFSET(U9),
        CTSTriggerLevel      WIDTH(U6) OFFSET(U10)
    ]
}

pub struct Uart {
    pub control1: UartControl1::Register,
    // control2: UartControl2::Register,
    // control3: UartControl3::Register,
    // control4: UartControl4::Register,
    pub tx: UartTX::Register,
    pub rx: UartRX::Register,
}

impl Uart {
    pub fn get(&self) -> Option<u8> {
        self.rx
            .get_field(UartRX::Data::Read)
            .map(|field| field.val() as u8)
    }

    pub fn put(&mut self, data: u8) {
        let checked = UartTX::Data::Field::new(data as u32).expect("uart data out of bounds");
        self.tx.modify(checked);
    }
}
