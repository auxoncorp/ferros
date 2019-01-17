use core::marker::PhantomData;
use crate::userland::{
    role, AssignedPageDirectory, Cap, Error, MappedPage, MappedPageTable, PhantomCap, UnmappedPage,
    UnmappedPageTable,
};
use sel4_sys::*;

// vspace related capability operations
impl Cap<AssignedPageDirectory, role::Local> {
    pub fn map_page_table(
        &mut self,
        page_table: Cap<UnmappedPageTable, role::Local>,
        virtual_address: usize,
    ) -> Result<Cap<MappedPageTable, role::Local>, Error> {
        // map the page table
        let err = unsafe {
            seL4_ARM_PageTable_Map(
                page_table.cptr,
                self.cptr,
                virtual_address,
                // TODO: JON! What do we write here? The default (according to
                // sel4_ appears to be pageCachable | parityEnabled)
                seL4_ARM_VMAttributes_seL4_ARM_PageCacheable
                    | seL4_ARM_VMAttributes_seL4_ARM_ParityEnabled, // | seL4_ARM_VMAttributes_seL4_ARM_ExecuteNever
            )
        };

        if err != 0 {
            return Err(Error::MapPageTable(err));
        }
        Ok(Cap {
            cptr: page_table.cptr,
            cap_data: PhantomCap::phantom_instance(),
            _role: PhantomData,
        })
    }

    pub fn map_page(
        &mut self,
        page: Cap<UnmappedPage, role::Local>,
        virtual_address: usize,
    ) -> Result<Cap<MappedPage, role::Local>, Error> {
        let err = unsafe {
            seL4_ARM_Page_Map(
                page.cptr,
                self.cptr,
                virtual_address,
                seL4_CapRights_new(0, 1, 1), // read/write
                // TODO: JON! What do we write here? The default (according to
                // sel4_ appears to be pageCachable | parityEnabled)
                seL4_ARM_VMAttributes_seL4_ARM_PageCacheable
                    | seL4_ARM_VMAttributes_seL4_ARM_ParityEnabled
                    // | seL4_ARM_VMAttributes_seL4_ARM_ExecuteNever,
            )
        };
        if err != 0 {
            return Err(Error::MapPage(err));
        }
        Ok(Cap {
            cptr: page.cptr,
            cap_data: PhantomCap::phantom_instance(),
            _role: PhantomData,
        })
    }
}

impl Cap<MappedPageTable, role::Local> {
    pub fn unmap(self) -> Result<Cap<UnmappedPageTable, role::Local>, Error> {
        let err = unsafe { seL4_ARM_PageTable_Unmap(self.cptr) };
        if err != 0 {
            return Err(Error::UnmapPageTable(err));
        }
        Ok(Cap {
            cptr: self.cptr,
            cap_data: PhantomCap::phantom_instance(),
            _role: PhantomData,
        })
    }
}

impl Cap<MappedPage, role::Local> {
    pub fn unmap(self) -> Result<Cap<UnmappedPage, role::Local>, Error> {
        let err = unsafe { seL4_ARM_Page_Unmap(self.cptr) };
        if err != 0 {
            return Err(Error::UnmapPage(err));
        }
        Ok(Cap {
            cptr: self.cptr,
            cap_data: PhantomCap::phantom_instance(),
            _role: PhantomData,
        })
    }
}