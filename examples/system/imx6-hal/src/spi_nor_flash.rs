//! SPI NOR FLASH

use crate::{
    embedded_hal::{blocking::spi::Transfer, digital::v2::OutputPin},
    gpio::{Output, PushPull, P3_19},
    pac::ecspi1::ECSPI1,
    spi::Spi,
};
use bitflags::bitflags;

/// 2 MiB
pub const FLASH_SIZE_BYTES: usize = 2 * 1024 * 1024;
/// 4 KiB
pub const ERASE_SIZE_BYTES: usize = 4096;
/// 256 byte pages
pub const PAGE_SIZE_BYTES: usize = 256;

pub type CsPin = P3_19<Output<PushPull>>;

#[derive(Copy, Clone, Eq, PartialEq, Ord, PartialOrd, Hash, Debug)]
pub enum Error {
    /// An SPI transfer failed.
    Spi,

    /// A GPIO could not be set.
    Gpio,

    /// Status register contained unexpected flags.
    ///
    /// This can happen when the chip is faulty, incorrectly connected, or the
    /// driver wasn't constructed or destructed properly (eg. while there is
    /// still a write in progress).
    UnexpectedStatus,
}

pub struct JedecId {
    bytes: [u8; 3],
}

impl JedecId {
    pub fn mfr_code(&self) -> u8 {
        self.bytes[0]
    }

    pub fn device_id(&self) -> u16 {
        u16::from_be_bytes([self.bytes[1], self.bytes[2]])
    }
}

bitflags! {
    /// Status register bits.
    pub struct Status: u8 {
        /// Erase or write in progress.
        const BUSY = 1 << 0;
        /// Status of the **W**rite **E**nable **L**atch.
        const WEL = 1 << 1;
        /// The 3 protection region bits.
        const PROT = 0b00011100;
        /// **S**tatus **R**egister **W**rite **D**isable bit.
        const SRWD = 1 << 7;
    }
}

enum Opcode {
    ReadStatus = 0x05,
    ReadJedecId = 0x9F,
    ReadFast = 0x0B,
    SectorErase = 0x20,
    WriteEnable = 0x06,
    WriteDisable = 0x04,
    PageProg = 0x02,
}

pub struct SpiNorFlash {
    spi: Spi<ECSPI1>,
    cs: CsPin,
}

impl SpiNorFlash {
    pub fn init(spi: Spi<ECSPI1>, cs: CsPin) -> Result<Self, Error> {
        let mut f = Self { spi, cs };
        let status = f.read_status()?;
        let id = f.read_jedec_id()?;
        log::trace!(
            "[flash] init status={:?} MFR=0x{:X} ID=0x{:X}",
            status,
            id.mfr_code(),
            id.device_id()
        );
        if !(status & (Status::BUSY | Status::WEL)).is_empty() {
            return Err(Error::UnexpectedStatus);
        }
        Ok(f)
    }

    pub fn read_status(&mut self) -> Result<Status, Error> {
        let mut cmd = [Opcode::ReadStatus as u8];
        let mut data = [0];
        self.command(&mut cmd, &mut data)?;
        Ok(Status::from_bits_truncate(data[0]))
    }

    pub fn read_jedec_id(&mut self) -> Result<JedecId, Error> {
        let mut cmd = [Opcode::ReadJedecId as u8];
        let mut data: [u8; 6] = [0; 6];
        self.command(&mut cmd, &mut data)?;
        Ok(JedecId {
            bytes: [data[0], data[1], data[2]],
        })
    }

    pub fn read(&mut self, addr: u32, buf: &mut [u8]) -> Result<(), Error> {
        let mut cmd = [
            Opcode::ReadFast as u8,
            (addr >> 16) as u8,
            (addr >> 8) as u8,
            addr as u8,
            0xFF,
        ];
        log::trace!("[flash] ReadFast {} bytes from 0x{:X}", buf.len(), addr);
        self.spi.reset();
        self.cs.set_low().map_err(|_| Error::Gpio)?;
        let mut spi_result = self.spi.transfer(&mut cmd);
        if spi_result.is_ok() {
            spi_result = self.spi.transfer(buf);
        }
        self.cs.set_high().map_err(|_| Error::Gpio)?;
        spi_result.map(|_| ()).map_err(|_| Error::Spi)
    }

    pub fn erase_sector(&mut self, addr: u32) -> Result<(), Error> {
        log::trace!("[flash] SectorErase 0x{:X}", addr);
        self.write_enable()?;
        let mut cmd = [
            Opcode::SectorErase as u8,
            (addr >> 16) as u8,
            (addr >> 8) as u8,
            addr as u8,
        ];
        self.command(&mut cmd, &mut [])?;
        self.wait_done()?;
        self.write_disable()?;
        Ok(())
    }

    pub fn write_page(&mut self, addr: u32, data: &mut [u8]) -> Result<(), Error> {
        log::trace!("[flash] PageProg 0x{:X} size {}", addr, data.len());
        self.write_enable()?;
        let mut cmd = [
            Opcode::PageProg as u8,
            (addr >> 16) as u8,
            (addr >> 8) as u8,
            addr as u8,
        ];
        self.spi.reset();
        self.cs.set_low().map_err(|_| Error::Gpio)?;
        let mut spi_result = self.spi.transfer(&mut cmd);
        if spi_result.is_ok() {
            spi_result = self.spi.transfer(data);
        }
        self.cs.set_high().map_err(|_| Error::Gpio)?;
        spi_result.map(|_| ()).map_err(|_| Error::Spi)?;
        self.wait_done()?;
        self.write_disable()?;
        Ok(())
    }

    fn command(&mut self, cmd: &mut [u8], data: &mut [u8]) -> Result<(), Error> {
        self.spi.reset();
        self.cs.set_low().map_err(|_| Error::Gpio)?;
        self.spi.transfer(cmd).map_err(|_| Error::Spi)?;
        if !data.is_empty() {
            self.spi.transfer(data).map_err(|_| Error::Spi)?;
        }
        self.cs.set_high().map_err(|_| Error::Gpio)?;
        Ok(())
    }

    fn write_enable(&mut self) -> Result<(), Error> {
        let mut cmd = [Opcode::WriteEnable as u8];
        self.command(&mut cmd, &mut [])?;
        log::trace!("[flash] write enabled");
        Ok(())
    }

    fn write_disable(&mut self) -> Result<(), Error> {
        let mut cmd = [Opcode::WriteDisable as u8];
        self.command(&mut cmd, &mut [])?;
        log::trace!("[flash] write disabled");
        Ok(())
    }

    fn wait_done(&mut self) -> Result<(), Error> {
        while self.read_status()?.contains(Status::BUSY) {}
        Ok(())
    }
}
