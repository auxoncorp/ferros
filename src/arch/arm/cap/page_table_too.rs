use selfe_sys::*;

use crate::cap::{role, Cap, CapType, DirectRetype, WCNodeSlots, WUntyped};
use crate::userland::CapRights;
use crate::vspace_too::{MappingError, Maps, PagingTop};

use super::page_too::Page;

#[derive(Debug)]
pub struct PageTable {}

impl Maps<Cap<Page, role::Local>> for Cap<PageTable, role::Local> {
    fn map_item<RootG, Root>(
        &mut self,
        page: &Cap<Page, role::Local>,
        addr: usize,
        root: &mut PagingTop<RootG, Root>,
        rights: CapRights,
        _ut: &mut WUntyped,
        _slots: &mut WCNodeSlots,
    ) -> Result<(), MappingError>
    where
        Root: Maps<RootG>,
        Root: CapType,
    {
        if is_aligned(addr) {
            match unsafe {
                seL4_ARM_Page_Map(
                    page.cptr,
                    addr,
                    root.layer.cptr,
                    seL4_CapRights_t::from(rights),
                    seL4_ARM_VMAttributes_seL4_ARM_PageCacheable
                        | seL4_ARM_VMAttributes_seL4_ARM_ParityEnabled,
                )
            } {
                // Really sorry about this random 6 here, but it is
                // the `seL4_FailedLookup` value. See
                // seL4/libsel4/include/sel4/errors.h
                // TODO(dan@auxon.io): Find a way to map between seL4
                // errors and a Rust error enum.
                6 => Err(MappingError::Overflow),
                0 => Ok(()),
                e => Err(MappingError::PageMapFailure(e)),
            }
        } else {
            Err(MappingError::AddrNotPageAligned)
        }
    }
}

impl CapType for PageTable {}

impl DirectRetype for PageTable {
    type SizeBits = super::super::PageDirectoryBits;
    fn sel4_type_id() -> usize {
        _object_seL4_ARM_PageTableObject as usize
    }
}

fn is_aligned(addr: usize) -> bool {
    addr % (1 << 12) == 0
}
