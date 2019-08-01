use selfe_sys::*;

use crate::cap::{page_state, DirectRetype, LocalCap, Page, PageTable};
use crate::error::{ErrorExt, SeL4Error};
use crate::userland::CapRights;

impl LocalCap<Page<page_state::Unmapped>> {
    pub(crate) unsafe fn unchecked_page_map(
        &self,
        addr: usize,
        root: &mut LocalCap<crate::arch::PagingRoot>,
        rights: CapRights,
        vm_attributes: seL4_ARM_VMAttributes,
    ) -> Result<(), SeL4Error> {
        seL4_ARM_Page_Map(
            self.cptr,
            root.cptr,
            addr,
            seL4_CapRights_t::from(rights),
            vm_attributes,
        )
        .as_result()
        .map_err(|e| SeL4Error::PageMap(e))
    }
}

impl DirectRetype for PageTable {
    type SizeBits = super::super::PageTableBits;
    fn sel4_type_id() -> usize {
        _object_seL4_ARM_PageTableObject as usize
    }
}
