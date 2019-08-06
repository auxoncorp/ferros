use selfe_sys::_mode_object_seL4_ARM_HugePageObject;

use crate::arch::cap::UnmappablePage;
use crate::cap::{CapType, DirectRetype, LocalCap, PhantomCap};

pub struct HugePage {}

impl HugePage {
    pub const TYPE_ID: usize = _mode_object_seL4_ARM_HugePageObject;
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

impl UnmappablePage for LocalCap<HugePage> {}
