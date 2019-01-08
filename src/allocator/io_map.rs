use super::{Allocator, Error};
use sel4_sys::*;

impl Allocator {
    pub fn io_map(&mut self, paddr: seL4_Word, size_bits: usize) -> Result<seL4_Word, Error> {
        let vaddr = self.vspace_new_pages_at(
            Some(paddr),
            // num_pages
            //1,
            (1 << size_bits) / (1 << seL4_PageBits),
            size_bits,
            unsafe { seL4_CapRights_new(1, 1, 1) },
            // no attributes for memory mapped devices
            0,
            true,
            None,
        )?;

        Ok(vaddr)
    }
}
