use selfe_sys::*;

use crate::cap::{page_state, DirectRetype, LocalCap, Page, PhantomCap};
use crate::userland::CapRights;
use selfe_wrap::error::{APIError, APIMethod, ErrorExt, PageMethod};

impl LocalCap<Page<page_state::Unmapped>> {
    pub(crate) unsafe fn unchecked_page_map(
        &self,
        addr: usize,
        root: &mut LocalCap<crate::arch::PagingRoot>,
        rights: CapRights,
        vm_attributes: seL4_ARM_VMAttributes,
    ) -> Result<(), APIError> {
        seL4_ARM_Page_Map(
            self.cptr,
            root.cptr,
            addr,
            seL4_CapRights_t::from(rights),
            vm_attributes,
        )
        .as_result()
        .map_err(|e| APIError::new(APIMethod::Page(PageMethod::Map), e))
    }
}

impl LocalCap<Page<page_state::Mapped>> {
    /// Keeping this non-public in order to restrict mapping operations to owners
    /// of a VSpace-related object
    pub(crate) fn unmap(self) -> Result<LocalCap<Page<page_state::Unmapped>>, APIError> {
        match unsafe { seL4_ARM_Page_Unmap(self.cptr) }.as_result() {
            Ok(_) => Ok(crate::cap::Cap {
                cptr: self.cptr,
                cap_data: Page {
                    state: page_state::Unmapped {},
                },
                _role: core::marker::PhantomData,
            }),
            Err(e) => Err(APIError::new(APIMethod::Page(PageMethod::Unmap), e)),
        }
    }
}

impl DirectRetype for Page<page_state::Unmapped> {
    type SizeBits = super::super::PageBits;
    fn sel4_type_id() -> usize {
        _object_seL4_ARM_SmallPageObject as usize
    }
}

impl PhantomCap for Page<page_state::Unmapped> {
    fn phantom_instance() -> Self {
        Page {
            state: page_state::Unmapped {},
        }
    }
}
