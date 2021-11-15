use selfe_sys::*;

use crate::cap::{page_state, DirectRetype, LocalCap, Page, PageState, PhantomCap};
use crate::error::{ErrorExt, SeL4Error};
use crate::typenum::Unsigned;
use crate::userland::CapRights;

impl<T: PageState> LocalCap<Page<T>> {
    pub(crate) fn paddr(&self) -> Result<usize, SeL4Error> {
        let res = unsafe { seL4_ARM_Page_GetAddress(self.cptr) };
        match (res.error as seL4_Error).as_result() {
            Ok(_) => Ok(res.paddr),
            Err(e) => Err(SeL4Error::PageGetAddress(e)),
        }
    }
}

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
        .map_err(SeL4Error::PageMap)
    }
}

impl LocalCap<Page<page_state::Mapped>> {
    /// Keeping this non-public in order to restrict mapping operations to
    /// owners of a VSpace-related object
    pub(crate) fn unmap(self) -> Result<LocalCap<Page<page_state::Unmapped>>, SeL4Error> {
        if self.rights().is_writable() {
            unsafe {
                seL4_ARM_Page_CleanInvalidate_Data(
                    self.cptr,
                    0x0000,
                    super::super::PageBytes::USIZE,
                )
            }
            .as_result()
            .map_err(SeL4Error::PageCleanInvalidateData)?;
        }

        match unsafe { seL4_ARM_Page_Unmap(self.cptr) }.as_result() {
            Ok(_) => Ok(crate::cap::Cap {
                cptr: self.cptr,
                cap_data: Page {
                    state: page_state::Unmapped {},
                },
                _role: core::marker::PhantomData,
            }),
            Err(e) => Err(SeL4Error::PageUnmap(e)),
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
