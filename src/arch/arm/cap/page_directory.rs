use selfe_sys::*;

use typenum::Unsigned;

use crate::cap::{CapType, DirectRetype, LocalCap, PhantomCap};
use crate::error::SeL4Error;
use crate::userland::CapRights;
use crate::vspace::{MappingError, Maps};

use super::super::{PageDirIndexBits, PageIndexBits, PageTableIndexBits};
use super::PageTable;

const PD_MASK: usize =
    (((1 << PageDirIndexBits::USIZE) - 1) << PageIndexBits::USIZE + PageTableIndexBits::USIZE);

#[derive(Debug)]
pub struct PageDirectory {}

impl Maps<PageTable> for PageDirectory {
    fn map_granule<RootG, Root>(
        &mut self,
        table: &LocalCap<PageTable>,
        addr: usize,
        root: &mut LocalCap<Root>,
        _rights: CapRights,
    ) -> Result<(), MappingError>
    where
        Root: Maps<RootG>,
        Root: CapType,
        RootG: CapType,
    {
        debug_println!(
            "PageDirectory::map_granule for dir {:?} mapping table: {:?}",
            self,
            table
        );
        match unsafe {
            seL4_ARM_PageTable_Map(
                table.cptr,
                root.cptr,
                addr & PD_MASK,
                seL4_ARM_VMAttributes_seL4_ARM_PageCacheable
                    | seL4_ARM_VMAttributes_seL4_ARM_ParityEnabled,
            )
        } {
            0 => Ok(()),
            6 => Err(MappingError::Overflow),
            e => Err(MappingError::IntermediateLayerFailure(
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
