use crate::arch::paging::PageBits;
use crate::userland::cache::{CacheOp, CacheableMemory};
use crate::userland::memory_region::{Address, MemoryRegion, MemoryRegionError};
use crate::userland::SeL4Error;
use selfe_sys::{
    seL4_ARM_PageDirectory_CleanInvalidate_Data, seL4_ARM_PageDirectory_Clean_Data,
    seL4_ARM_PageDirectory_Invalidate_Data,
};
use typenum::Unsigned;

impl<VAddr: Unsigned, PAddr: Unsigned, SizeBytes: Unsigned> CacheableMemory
    for MemoryRegion<VAddr, PAddr, SizeBytes>
{
    type Error = MemoryRegionError;

    fn cache_op(&mut self, op: CacheOp, addr: usize, size: usize) -> Result<(), Self::Error> {
        let end = addr + size;
        let mut cur = addr;

        while cur < end {
            let mut top = round_up(cur + 1, PageBits::USIZE);
            if top > end {
                top = end;
            }

            match op {
                CacheOp::CleanData => self.clean_data(cur, top)?,
                CacheOp::InvalidateData => self.invalidate_data(cur, top)?,
                CacheOp::CleanInvalidateData => self.clean_invalidate_data(cur, top)?,
            }

            cur = top;
        }

        Ok(())
    }

    fn clean_data(&mut self, start_addr: usize, end_addr: usize) -> Result<(), Self::Error> {
        let cap = if let Some(c) = &self.vspace_pagedir {
            c.cptr
        } else {
            return Err(MemoryRegionError::NotSupported);
        };

        if self.contains_range(Address::Virtual(start_addr), end_addr - start_addr) == false {
            return Err(MemoryRegionError::OutOfBounds);
        }

        let err = unsafe { seL4_ARM_PageDirectory_Clean_Data(cap, start_addr, end_addr) };

        if err != 0 {
            return Err(SeL4Error::PageDirCleanData(err).into());
        }

        Ok(())
    }

    fn invalidate_data(&mut self, start_addr: usize, end_addr: usize) -> Result<(), Self::Error> {
        let cap = if let Some(c) = &self.vspace_pagedir {
            c.cptr
        } else {
            return Err(MemoryRegionError::NotSupported);
        };

        if self.contains_range(Address::Virtual(start_addr), end_addr - start_addr) == false {
            return Err(MemoryRegionError::OutOfBounds);
        }

        let err = unsafe { seL4_ARM_PageDirectory_Invalidate_Data(cap, start_addr, end_addr) };

        if err != 0 {
            return Err(SeL4Error::PageDirInvalidateData(err).into());
        }

        Ok(())
    }

    fn clean_invalidate_data(
        &mut self,
        start_addr: usize,
        end_addr: usize,
    ) -> Result<(), Self::Error> {
        let cap = if let Some(c) = &self.vspace_pagedir {
            c.cptr
        } else {
            return Err(MemoryRegionError::NotSupported);
        };

        if self.contains_range(Address::Virtual(start_addr), end_addr - start_addr) == false {
            return Err(MemoryRegionError::OutOfBounds);
        }

        let err = unsafe { seL4_ARM_PageDirectory_CleanInvalidate_Data(cap, start_addr, end_addr) };

        if err != 0 {
            return Err(SeL4Error::PageDirCleanInvalidateData(err).into());
        }

        Ok(())
    }
}

fn round_up(val: usize, base: usize) -> usize {
    val + if val % base == 0 {
        0
    } else {
        base - (val % base)
    }
}
