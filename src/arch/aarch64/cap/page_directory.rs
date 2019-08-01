use selfe_sys::*;

use typenum::Unsigned;

use crate::cap::{CapType, DirectRetype, LocalCap, PhantomCap};
use crate::error::{ErrorExt, KernelError, SeL4Error};
use crate::userland::CapRights;
use crate::vspace::{MappingError, Maps};

use super::super::{PageDirIndexBits, PageIndexBits, PageTableIndexBits, PagingRoot};
use super::PageTable;

const PD_MASK: usize = !((1 << PageIndexBits::USIZE) - 1);

#[derive(Debug)]
pub struct PageDirectory {}

impl Maps<PageTable> for PageDirectory {
    fn map_granule(
        &mut self,
        table: &LocalCap<PageTable>,
        addr: usize,
        root: &mut LocalCap<PagingRoot>,
        _rights: CapRights,
        vm_attributes: seL4_ARM_VMAttributes,
    ) -> Result<(), MappingError> {
        match unsafe {
            seL4_ARM_PageTable_Map(table.cptr, root.cptr, addr & PD_MASK, vm_attributes)
        }
        .as_result()
        {
            Ok(_) => Ok(()),
            Err(KernelError::FailedLookup) => Err(MappingError::Overflow),
            Err(e) => Err(MappingError::IntermediateLayerFailure(
                SeL4Error::PageTableMap(e),
            )),
        }
    }
}

impl CapType for PageDirectory {}
impl PhantomCap for PageDirectory {
    fn phantom_instance() -> Self {
        PageDirectory {}
    }
}

impl DirectRetype for PageDirectory {
    type SizeBits = super::super::PageDirectoryBits;
    fn sel4_type_id() -> usize {
        _object_seL4_ARM_PageDirectoryObject as usize
    }
}
