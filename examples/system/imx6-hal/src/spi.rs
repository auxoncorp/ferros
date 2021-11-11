//! SPI driver based on the u-boot ECSPI driver
// TODO - can probably cleanup the Transfer impl to not require 2 part
// operations

use crate::asm;
use crate::pac::{ecspi1::*, typenum};
use core::cmp;
use embedded_hal::blocking::spi;
pub use embedded_hal::spi::{Mode, Phase, Polarity};
use num::integer::Integer;

const FIFO_SIZE_WORDS: usize = 64;
const FIFO_SIZE_BYTES: usize = FIFO_SIZE_WORDS * 4;

/// SPI error
#[non_exhaustive]
#[derive(Debug, Eq, PartialEq, Copy, Clone)]
pub enum Error {
    /// Overrun occurred
    Overrun,
    /// This driver only supports transfers of up to `FIFO_SIZE_BYTES`
    TooMuchData,
}

pub struct Spi<SPI> {
    spi: SPI,
}

impl Spi<ECSPI1> {
    pub fn new(spi: ECSPI1) -> Self {
        log::trace!("[ECSPI1] init");
        let mut spi = Spi { spi };
        spi.reset();
        spi
    }

    pub fn reset(&mut self) {
        self.spi.ctl.modify(
            Control::Enable::Clear
                + Control::Channel0Mode::ModeMaster
                + Control::Channel1Mode::ModeMaster
                + Control::Channel2Mode::ModeMaster
                + Control::Channel3Mode::ModeMaster,
        );

        self.spi.ctl.modify(
            Control::Enable::Set
                + Control::HardwareTrigger::Clear
                + Control::Exchange::Clear
                + Control::StartModeControl::Clear
                + Control::PostDivider::Field::checked::<typenum::U0>()
                + Control::PreDivider::Field::checked::<typenum::U2>()
                + Control::DataReadyControl::Any
                + Control::ChannelSelect::ChipSelect0
                + Control::BurstLength::Field::checked::<typenum::U0>(),
        );

        self.spi.cfg.modify(
            Config::Channel0Phase::Phase0
                + Config::Channel0Polarity::ActiveHigh
                + Config::Channel0WaveFromSelect::Clear
                + Config::Channel0SSPolarity::ActiveLow
                + Config::Channel0DataCtl::StayHigh
                + Config::Channel0SclkCtl::StayLow,
        );

        self.spi
            .int
            .modify(Interrupt::Bits::Field::checked::<typenum::U0>());

        self.spi
            .status
            .modify(Status::RxFifoOverflow::Set + Status::TransferComplete::Set);

        self.spi.period.modify(Period::ClockSource::RefClock);

        log::trace!(
            "[ECSPI1] ctl=0x{:04X}, cfg=0x{:04X}, period=0x{:04X}",
            self.spi.ctl.read(),
            self.spi.cfg.read(),
            self.spi.period.read()
        );
    }
}

impl spi::Transfer<u8> for Spi<ECSPI1> {
    type Error = Error;

    fn transfer<'w>(&mut self, words: &'w mut [u8]) -> Result<&'w [u8], Self::Error> {
        let mut n_bytes = words.len();
        let n_bits = n_bytes * 8;
        log::trace!("[ECSPI1] transfer len {} bit_len {}", n_bytes, n_bits);

        if n_bytes > FIFO_SIZE_BYTES {
            log::error!("[ECSPI1] transfer would overflow the fifo");
            return Err(Error::TooMuchData);
        }

        self.spi.ctl.modify(
            Control::Enable::Set + Control::BurstLength::Field::new((n_bits - 1) as u32).unwrap(),
        );

        self.spi
            .status
            .modify(Status::RxFifoOverflow::Set + Status::TransferComplete::Set);

        // The SPI controller works only with words,
        // check if less than a word is sent.
        // Access to the FIFO is only 32 bit
        let mut byte_index = 0;
        let mut data: u32 = 0;

        if n_bits % 32 != 0 {
            let cnt = (n_bits % 32) / 8;
            for _ in 0..cnt {
                data = (data << 8) | u32::from(words[byte_index]);
                byte_index += 1;
            }
            unsafe { self.spi.tx.write(data) };
            n_bytes -= cnt;
        }

        while n_bytes > 0 {
            data = 0;
            for _ in 0..4 {
                data = (data << 8) | u32::from(words[byte_index]);
                byte_index += 1;
            }
            unsafe { self.spi.tx.write(data) };
            n_bytes -= 4;
        }

        // FIFO is written, now starts the transfer setting the XCH bit
        self.spi.ctl.modify(Control::Exchange::Set);

        // Wait until the TC (Transfer completed) bit is set
        while !self.spi.status.is_set(Status::TransferComplete::Set) {
            asm::nop();
        }

        // Transfer completed, clear any pending request
        self.spi
            .status
            .modify(Status::RxFifoOverflow::Set + Status::TransferComplete::Set);

        n_bytes = n_bits.div_ceil(&8);
        byte_index = 0;

        if n_bits % 32 != 0 {
            data = self.spi.rx.read();
            let cnt = (n_bits % 32) / 8;
            data = data.to_be() >> ((4 - cnt) * 8);
            (&mut words[byte_index..byte_index + cnt]).copy_from_slice(&data.to_ne_bytes()[..cnt]);
            byte_index += cnt;
            n_bytes -= cnt;
        }

        while n_bytes > 0 {
            let tmp = self.spi.rx.read();
            data = tmp.to_be();
            let cnt = cmp::min(n_bytes, 4);
            (&mut words[byte_index..byte_index + cnt]).copy_from_slice(&data.to_ne_bytes()[..cnt]);
            byte_index += cnt;
            n_bytes -= cnt;
        }

        if self.spi.status.is_set(Status::RxFifoOverflow::Set) {
            Err(Error::Overrun)
        } else {
            Ok(words)
        }
    }
}
