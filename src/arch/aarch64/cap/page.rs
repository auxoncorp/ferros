use selfe_sys::*;

use crate::cap::{CapType, DirectRetype};

pub struct Page {
    vaddr: usize,
}

impl CapType for Page {}

impl DirectRetype for Page {
    type SizeBits = super::super::PageBits;
    fn sel4_type_id() -> usize {
        _object_seL4_ARM_SmallPageObject as usize
    }
}
