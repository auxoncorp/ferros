use selfe_sys::*;

use crate::cap::{DirectRetype, LocalCap, Page, PhantomCap};
use crate::error::{ErrorExt, SeL4Error};

impl LocalCap<Page> {
    /// Keeping this non-public in order to restrict mapping operations to owners
    /// of a VSpace-related object
    pub(crate) fn unmap(self) -> Result<LocalCap<Page<page_state::Unmapped>>, SeL4Error> {
        match unsafe { seL4_ARM_Page_Unmap(self.cptr) }.as_result() {
            Ok(_) => Ok(crate::cap::Cap {
                cptr: self.cptr,
                cap_data: Page {},
                _role: core::marker::PhantomData,
            }),
            Err(e) => Err(SeL4Error::PageUnmap(e)),
        }
    }
}

impl DirectRetype for Page {
    type SizeBits = super::super::PageBits;
    fn sel4_type_id() -> usize {
        _object_seL4_ARM_SmallPageObject as usize
    }
}
