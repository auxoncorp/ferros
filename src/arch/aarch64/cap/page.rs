use selfe_sys::*;

use crate::cap::{
    page_state, CapType, CopyAliasable, DirectRetype, InternalASID, LocalCap, Movable, Page,
    PageState, PhantomCap,
};
use crate::error::{ErrorExt, SeL4Error};

impl LocalCap<Page<page_state::Mapped>> {
    pub fn vaddr(&self) -> usize {
        self.cap_data.state.vaddr
    }
    pub(crate) fn asid(&self) -> InternalASID {
        self.cap_data.state.asid
    }

    /// Keeping this non-public in order to restrict mapping operations to owners
    /// of a VSpace-related object
    pub(crate) fn unmap(self) -> Result<LocalCap<Page<page_state::Unmapped>>, SeL4Error> {
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

impl<State: PageState> CapType for Page<State> {}

impl DirectRetype for Page<page_state::Unmapped> {
    type SizeBits = super::super::PageBits;
    fn sel4_type_id() -> usize {
        _object_seL4_ARM_SmallPageObject as usize
    }
}

impl CopyAliasable for Page<page_state::Unmapped> {
    type CopyOutput = Self;
}

impl CopyAliasable for Page<page_state::Mapped> {
    // TODO - revisit whether CopyOutput here should have page_state::Mapped
    // and if so, how do we adjust this idiom (or add a new one) to support CapType instance cloning
    // for non-phantom CapTypes since there's a clear possibility for mismatch between visible
    // runtime state and in-kernel state
    type CopyOutput = Page<page_state::Unmapped>;
}
impl<State: PageState> Movable for Page<State> {}

impl PhantomCap for Page<page_state::Unmapped> {
    fn phantom_instance() -> Self {
        Page {
            state: page_state::Unmapped {},
        }
    }
}
