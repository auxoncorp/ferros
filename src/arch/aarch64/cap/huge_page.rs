use selfe_sys::_mode_object_seL4_ARM_HugePageObject;

use crate::cap::{CapType, DirectRetype, PhantomCap};

pub struct HugePage {}

impl HugePage {
    pub const TYPE_ID: usize = _mode_object_seL4_ARM_HugePageObject as usize;
}

impl CapType for HugePage {}

impl DirectRetype for HugePage {
    type SizeBits = crate::arch::HugePageBits;
    fn sel4_type_id() -> usize {
        Self::TYPE_ID
    }
}

impl PhantomCap for HugePage {
    fn phantom_instance() -> Self {
        HugePage {}
    }
}
