#![no_std]

use ferros::cap::{role, CNodeRole};
use ferros::userland::{Caller, InterruptConsumer, Producer, RetypeForSetup};
use ferros::vspace::{shared_status, MappedMemoryRegion};
use imx6_hal::pac::{
    typenum::{op, U1, U12},
    uart1::{self, UART1},
};
use net_types::IpcUdpTransmitBuffer;

/// Expected badge value on IRQ notifications
pub type IrqBadgeBits = uart1::Irq;

/// 4K console buffer
pub type ConsoleBufferSizeBits = U12;
pub type ConsoleBufferSizeBytes = op! { U1 << ConsoleBufferSizeBits };

#[repr(C)]
pub struct ProcParams<Role: CNodeRole> {
    /// Console UART/serial
    pub uart: UART1,

    /// Interrupt consumer for the console UART
    pub int_consumer: InterruptConsumer<uart1::Irq, Role>,

    /// IPC to the storage driver
    pub storage_caller: Caller<
        persistent_storage::Request,
        Result<persistent_storage::Response, persistent_storage::ErrorCode>,
        Role,
    >,

    /// Producer of UDP messages destined to the TCP/IP driver
    pub udp_producer: Producer<Role, IpcUdpTransmitBuffer>,

    /// Console buffer memory
    pub console_buffer: MappedMemoryRegion<ConsoleBufferSizeBits, shared_status::Exclusive>,
}

impl RetypeForSetup for ProcParams<role::Local> {
    type Output = ProcParams<role::Child>;
}
