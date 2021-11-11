use core::cell::RefCell;
use imx6_hal::spi_nor_flash::{
    SpiNorFlash as Flash, ERASE_SIZE_BYTES, FLASH_SIZE_BYTES, PAGE_SIZE_BYTES,
};
use static_assertions::const_assert_eq;
use tickv::{ErrorCode, FlashController};

// TODO
// put some checks in the linker script or somewhere to check the binary
// doesn't run into the reserved region

/// Reserve the last sector of flash for persistent storage
const REGION_BASE_ADDR: usize = FLASH_SIZE_BYTES - ERASE_SIZE_BYTES;
const_assert_eq!(REGION_BASE_ADDR & 0xFFF, 0);

pub struct SpiNorFlashController<'a> {
    flash: RefCell<Flash>,
    scratchpad: RefCell<&'a mut [u8]>,
}

impl<'a> SpiNorFlashController<'a> {
    pub fn new(flash: Flash, scratchpad: &'a mut [u8]) -> Result<Self, ErrorCode> {
        if scratchpad.len() < PAGE_SIZE_BYTES {
            Err(ErrorCode::BufferTooSmall(PAGE_SIZE_BYTES))
        } else {
            log::trace!(
                "[tickv] SpiNorFlashController base address=0x{:X}",
                REGION_BASE_ADDR,
            );
            Ok(SpiNorFlashController {
                flash: RefCell::new(flash),
                scratchpad: RefCell::new(scratchpad),
            })
        }
    }
}

impl<'a> FlashController<ERASE_SIZE_BYTES> for SpiNorFlashController<'a> {
    fn read_region(
        &self,
        region_number: usize,
        offset: usize,
        buf: &mut [u8; ERASE_SIZE_BYTES],
    ) -> Result<(), ErrorCode> {
        log::trace!(
            "[tickv] read region number={} offset=0x{:X}",
            region_number,
            offset
        );
        let mut flash = self.flash.borrow_mut();
        let base_addr = REGION_BASE_ADDR + (region_number + offset);
        for (c, chunk) in buf.chunks_mut(PAGE_SIZE_BYTES).enumerate() {
            let addr = (base_addr + (c * PAGE_SIZE_BYTES)) as u32;
            flash.read(addr, chunk).map_err(|_| ErrorCode::ReadFail)?;
        }
        Ok(())
    }

    fn write(&self, address: usize, buf: &[u8]) -> Result<(), ErrorCode> {
        log::trace!("[tickv] write address=0x{:X} len={}", address, buf.len());
        let mut flash = self.flash.borrow_mut();
        let mut scratchpad = self.scratchpad.borrow_mut();
        for (c, chunk) in buf.chunks(PAGE_SIZE_BYTES).enumerate() {
            let addr = (REGION_BASE_ADDR + (address + (c * PAGE_SIZE_BYTES))) as u32;
            let len = chunk.len();
            if len < PAGE_SIZE_BYTES {
                // TODO - check page-aligned address
                let dst = &mut scratchpad[..PAGE_SIZE_BYTES];
                flash.read(addr, dst).map_err(|_| ErrorCode::ReadFail)?;
                dst[..len].copy_from_slice(chunk);
                flash
                    .write_page(addr, dst)
                    .map_err(|_| ErrorCode::WriteFail)?;
            } else {
                scratchpad[..len].copy_from_slice(chunk);
                flash
                    .write_page(addr, &mut scratchpad[..len])
                    .map_err(|_| ErrorCode::WriteFail)?;
            }
        }
        Ok(())
    }

    fn erase_region(&self, region_number: usize) -> Result<(), ErrorCode> {
        log::trace!("[tickv] erase region number={}", region_number);
        let mut flash = self.flash.borrow_mut();
        flash
            .erase_sector((REGION_BASE_ADDR + region_number) as u32)
            .map_err(|_| ErrorCode::EraseFail)?;
        Ok(())
    }
}
