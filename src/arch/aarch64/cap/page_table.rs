use selfe_sys::*;

use crate::cap::{DirectRetype, PageTable};

impl DirectRetype for PageTable {
    type SizeBits = super::super::PageTableBits;
    fn sel4_type_id() -> usize {
        _object_seL4_ARM_PageTableObject as usize
    }
}
