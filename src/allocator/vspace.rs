// NOTE: this is not a proper vspace impl, just a testing area for now
// TODO
// - device support
// - derive clone for CapRights
// - get_cap(vaddr) fn to replace awkward Option<&mut seL4_CPtr> usage

// https://github.com/seL4/seL4_libs/blob/master/libsel4vspace/include/vspace/vspace.h
// https://github.com/seL4/seL4_libs/blob/master/libsel4utils/src/vspace/vspace.c
// https://github.com/seL4/seL4_libs/blob/master/libsel4utils/src/vspace/bootstrap.c
// https://github.com/seL4/seL4_libs/blob/master/libsel4vspace/src/sel4_arch/aarch32/mapping.c

// https://github.com/seL4/seL4_libs/blob/master/libsel4platsupport/src/io.c#L206

use super::{Allocator, Error, VSPACE_START};
use sel4_sys::*;

impl Allocator {
    pub fn bootstrap_vspace(&mut self, pd_cap: seL4_CPtr) -> Result<(), Error> {
        // set our vspace root page directory
        self.page_directory = pd_cap;
        self.last_allocated = VSPACE_START;
        Ok(())
    }

    pub fn vspace_new_ipc_buffer(
        &mut self,
        cap: Option<&mut seL4_CPtr>,
    ) -> Result<seL4_Word, Error> {
        self.vspace_new_pages(
            1,
            seL4_PageBits as _,
            unsafe { seL4_CapRights_new(1, 1, 1) },
            seL4_ARM_VMAttributes_seL4_ARM_Default_VMAttributes,
            cap,
        )
    }

    pub fn vspace_new_pages(
        &mut self,
        num_pages: usize,
        size_bits: usize,
        _rights: seL4_CapRights,
        cache_attributes: seL4_ARM_VMAttributes,
        cap: Option<&mut seL4_CPtr>,
    ) -> Result<seL4_Word, Error> {
        self.vspace_new_pages_at(
            None,
            num_pages,
            size_bits,
            //_rights,
            unsafe { seL4_CapRights_new(1, 1, 1) },
            cache_attributes,
            false,
            cap,
        )
    }

    // this doesn't work for multiple pages yet
    pub fn vspace_new_pages_at(
        &mut self,
        paddr: Option<seL4_Word>,
        num_pages: usize,
        size_bits: usize,
        _rights: seL4_CapRights,
        cache_attributes: seL4_ARM_VMAttributes,
        _can_use_dev: bool,
        cap: Option<&mut seL4_CPtr>,
    ) -> Result<(seL4_Word), Error> {
        let vaddr = self.last_allocated;
        let mut page_vaddr = vaddr;
        let mut first_cap: seL4_CPtr = 0;

        for page in 0..num_pages {
            let frame_obj = if let Some(paddr) = paddr {
                if page == 0 {
                    self.vka_alloc_frame_at(size_bits, paddr)?
                } else {
                    self.vka_alloc_frame(size_bits)?
                }
            } else {
                self.vka_alloc_frame(size_bits)?
            };

            self.map_page(
                frame_obj.cptr,
                page_vaddr,
                //rights,
                unsafe { seL4_CapRights_new(1, 1, 1) },
                cache_attributes,
            )?;

            if page == 0 {
                first_cap = frame_obj.cptr;
            }

            page_vaddr += 1 << size_bits;
        }

        // provide cap to the first frame
        if let Some(cap) = cap {
            *cap = first_cap;
        }

        self.last_allocated += num_pages as seL4_Word * (1 << size_bits) as seL4_Word;

        Ok(vaddr)
    }

    fn map_page(
        &mut self,
        cap: seL4_CPtr,
        vaddr: seL4_Word,
        _rights: seL4_CapRights,
        cache_attributes: seL4_ARM_VMAttributes,
    ) -> Result<(), Error> {
        let map_err: seL4_Error = unsafe {
            seL4_ARM_Page_Map(
                cap,
                self.page_directory,
                vaddr,
                //rights,
                seL4_CapRights_new(1, 1, 1),
                cache_attributes,
            )
        };

        if map_err != 0 {
            // create a page table
            // TODO - is leaky
            let pt_obj = self.vka_alloc_page_table()?;
            self.page_table = pt_obj.cptr;

            // map the page table
            let err: seL4_Error = unsafe {
                seL4_ARM_PageTable_Map(
                    self.page_table,
                    self.page_directory,
                    vaddr,
                    cache_attributes,
                )
            };

            if err != 0 {
                return Err(Error::Other);
            }

            // map the frame in
            let err: seL4_Error = unsafe {
                seL4_ARM_Page_Map(
                    cap,
                    self.page_directory,
                    vaddr,
                    //rights,
                    seL4_CapRights_new(1, 1, 1),
                    cache_attributes,
                )
            };

            if err != 0 {
                return Err(Error::Other);
            }
        }

        Ok(())
    }
}
