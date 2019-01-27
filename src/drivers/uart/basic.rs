use core::mem;
use core::ptr;
use crate::userland::{role, CNodeRole, Responder, RetypeForSetup};

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
    GetByte,
    PutByte(u8),
}

#[derive(Debug)]
pub enum UARTResponse {
    GotByte(u8),
    WroteByte,
    Error,
}

// UART Receiver Register
mod URXD {
    pub const OFFSET: usize = 0x00;
    pub const RX_DATA: usize = (0xFF << 0);
}

// UART Transmitter Register
#[rustfmt::skip]
mod UTXD {
    pub const OFFSET: usize = 0x40;
}

// UART Status Register 2
#[rustfmt::skip]
mod USR2 {
    pub const OFFSET: usize = 0x98;
    pub const TXFE   : usize = (1 << 14); // Transmit buffer FIFO empty
    pub const RDR    : usize = (1 << 0);  // Recv data ready
}

unsafe fn get_byte(usr2: *const usize, urxd: *const usize) -> u8 {
    loop {
        if (ptr::read_volatile(usr2) & USR2::RDR) != 0 {
            return (ptr::read_volatile(urxd) & URXD::RX_DATA) as u8;
        }
    }
}

unsafe fn put_byte(value: u8, usr2: *const usize, utxd: *mut usize) {
    loop {
        if (ptr::read_volatile(usr2) & USR2::TXFE) != 0 {
            ptr::write_volatile(utxd, value as usize);
            return;
        }
    }
}

pub extern "C" fn run(config: UARTConfig<role::Local>) {
    debug_println!(
        "Starting UART driver for 0x{:08x}",
        config.register_base_addr
    );

    let usr2: *const usize = unsafe { mem::transmute(config.register_base_addr + USR2::OFFSET) };
    let urxd: *const usize = unsafe { mem::transmute(config.register_base_addr + URXD::OFFSET) };
    let utxd: *mut usize = unsafe { mem::transmute(config.register_base_addr + UTXD::OFFSET) };

    config
        .responder
        .reply_recv(move |req| {
            use self::UARTCommand::*;
            match req {
                GetByte => UARTResponse::GotByte(unsafe { get_byte(usr2, urxd) }),
                PutByte(b) => {
                    unsafe { put_byte(*b, usr2, utxd) };
                    UARTResponse::WroteByte
                }
            }
        })
        .expect("Could not set up a reply_recv");
}
