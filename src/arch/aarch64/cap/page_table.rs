use selfe_sys::*;

use crate::cap::{CapType, DirectRetype, LocalCap, PhantomCap};
use crate::error::SeL4Error;
use crate::userland::CapRights;
use crate::vspace::{MappingError, Maps};

use super::{page_state, Page};

#[derive(Debug)]
pub struct PageTable {}

impl Maps<Page<page_state::Unmapped>> for PageTable {
    fn map_granule<RootG, Root>(
        &mut self,
        page: &LocalCap<Page<page_state::Unmapped>>,
        addr: usize,
        root: &mut LocalCap<Root>,
        rights: CapRights,
    ) -> Result<(), MappingError>
    where
        Root: Maps<RootG>,
        Root: CapType,
        RootG: CapType,
    {
        if is_aligned(addr) {
            match unsafe {
                seL4_ARM_Page_Map(
                    page.cptr,
                    root.cptr,
                    addr,
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
                e => Err(MappingError::PageMapFailure(SeL4Error::PageMap(e))),
            }
        } else {
            Err(MappingError::AddrNotPageAligned)
        }
    }
}

impl CapType for PageTable {}
impl PhantomCap for PageTable {
    fn phantom_instance() -> Self {
        PageTable {}
    }
}

impl DirectRetype for PageTable {
    type SizeBits = super::super::PageTableBits;
    fn sel4_type_id() -> usize {
        _object_seL4_ARM_PageTableObject as usize
    }
}

fn is_aligned(addr: usize) -> bool {
    addr % (1 << 12) == 0
}
