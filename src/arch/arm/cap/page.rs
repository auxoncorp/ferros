use selfe_sys::*;

use crate::cap::{memory_kind, page_state, DirectRetype, LocalCap, MemoryKind, Page};
use crate::error::{ErrorExt, SeL4Error};

impl DirectRetype for Page<page_state::Unmapped, memory_kind::General> {
    type SizeBits = super::super::PageBits;
    fn sel4_type_id() -> usize {
        _object_seL4_ARM_SmallPageObject as usize
    }
}

impl<MemKind: MemoryKind> LocalCap<Page<page_state::Mapped, MemKind>> {
    /// Keeping this non-public in order to restrict mapping operations to owners
    /// of a VSpace-related object
    pub(crate) fn unmap(self) -> Result<LocalCap<Page<page_state::Unmapped, MemKind>>, SeL4Error> {
        match unsafe { seL4_ARM_Page_Unmap(self.cptr) }.as_result() {
            Ok(_) => Ok(crate::cap::Cap {
                cptr: self.cptr,
                cap_data: Page {
                    state: page_state::Unmapped {},
                    memory_kind: self.cap_data.memory_kind,
                },
                _role: core::marker::PhantomData,
            }),
            Err(e) => Err(SeL4Error::PageUnmap(e)),
        }
    }
}
