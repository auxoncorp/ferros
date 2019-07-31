use selfe_sys::_object_seL4_ARM_LargePageObject;

use crate::arch::cap::UnmappablePage;
use crate::cap::{CapType, DirectRetype, LocalCap, PhantomCap};

pub struct LargePage {}

impl LargePage {
    pub const TYPE_ID: usize = _object_seL4_ARM_LargePageObject;
}

impl CapType for LargePage {}

impl DirectRetype for LargePage {
    type SizeBits = crate::arch::LargePageBits;
    fn sel4_type_id() -> usize {
        Self::TYPE_ID
    }
}

impl PhantomCap for LargePage {
    fn phantom_instance() -> Self {
        LargePage {}
    }
}

impl UnmappablePage for LocalCap<LargePage> {}
