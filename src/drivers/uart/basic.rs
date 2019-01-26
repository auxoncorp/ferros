use core::marker::PhantomData;
use crate::micro_alloc;
use crate::pow::Pow;
use crate::userland::{
    call_channel, role, root_cnode, setup_fault_endpoint_pair, spawn, BootInfo, CNode, CNodeRole,
    Caller, Cap, Endpoint, FaultSink, LocalCap, MappedPage, Responder, RetypeForSetup,
    UnmappedPageTable, Untyped,
};
use sel4_sys::*;
use typenum::operator_aliases::Diff;
use typenum::{U12, U2, U20, U4096, U6};

// driver config
#[derive(Debug)]
pub struct UARTConfig<Role: CNodeRole> {
    pub register_base_addr: usize,
    pub responder: Responder<UARTCommand, UARTResponse, Role>,
}

impl RetypeForSetup for UARTConfig<role::Local> {
    type Output = UARTConfig<role::Child>;
}

// driver api
#[derive(Debug)]
pub enum UARTCommand {
    GetChar,
    PutChar(char),
}

#[derive(Debug)]
pub enum UARTResponse {
    GotChar(char),
    WroteChar,
    Error,
}

// uart reg offsets
const URXD: usize = 0x00; /* UART Receiver Register */
const UTXD: usize = 0x40; /* UART Transmitter Register */
const UCR1: usize = 0x80; /* UART Control Register 1 */
const UCR2: usize = 0x84; /* UART Control Register 2 */
const UCR3: usize = 0x88; /* UART Control Register 3 */
const UCR4: usize = 0x8c; /* UART Control Register 4 */
const UFCR: usize = 0x90; /* UART FIFO Control Register */
const USR1: usize = 0x94; /* UART Status Register 1 */
const USR2: usize = 0x98; /* UART Status Register 2 */
const UESC: usize = 0x9c; /* UART Escape Character Register */
const UTIM: usize = 0xa0; /* UART Escape Timer Register */
const UBIR: usize = 0xa4; /* UART BRM Incremental Register */
const UBMR: usize = 0xa8; /* UART BRM Modulator Register */
const UBRC: usize = 0xac; /* UART Baud Rate Counter Register */
const ONEMS: usize = 0xb0; /* UART One Millisecond Register */
const UTS: usize = 0xb4; /* UART Test Register */

pub extern "C" fn run(config: UARTConfig<role::Local>) {
    debug_println!(
        "Starting UART driver for 0x{:08x}",
        config.register_base_addr
    );

    config
        .responder
        .reply_recv(move |req| {
            use self::UARTCommand::*;
            match req {
                GetChar => unimplemented!(),
                PutChar(c) => unimplemented!(),
            }
        })
        .expect("Could not set up a reply_recv");
}

// void
//     putDebugChar(unsigned char c)
// {
//     while (!(*UART_REG(USR2) & BIT(UART_SR2_TXFIFO_EMPTY)));
//     *UART_REG(UTXD) = c;
// }
// #endif

// unsigned char
//     getDebugChar(void)
// {
//     while (!(*UART_REG(USR2) & BIT(UART_SR2_RXFIFO_RDR)));
//     return *UART_REG(URXD);
// }
