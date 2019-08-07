use selfe_sys::*;

use crate::cap::{granule_state, DirectRetype, GranuleState, LocalCap, Page, PhantomCap};
use crate::error::{ErrorExt, SeL4Error};
use crate::userland::CapRights;

impl<State: GranuleState> Page<State> {
    pub(crate) const TYPE_ID: usize = _object_seL4_ARM_SmallPageObject as usize;
}

impl DirectRetype for Page<granule_state::Unmapped> {
    type SizeBits = super::super::PageBits;
    fn sel4_type_id() -> usize {
        Self::TYPE_ID
    }
}

impl PhantomCap for Page<granule_state::Unmapped> {
    fn phantom_instance() -> Self {
        Page {
            state: granule_state::Unmapped {},
        }
    }
}
