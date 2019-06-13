use selfe_sys::*;

use typenum::Unsigned;

use crate::cap::{CapType, DirectRetype, LocalCap, PhantomCap};
use crate::error::SeL4Error;
use crate::userland::CapRights;
use crate::vspace::{MappingError, Maps};

use super::super::{
    PageDirIndexBits, PageGlobalDirIndexBits, PageIndexBits, PageTableIndexBits,
    PageUpperDirIndexBits,
};
use super::PageUpperDirectory;

const GD_MASK: usize = (((1 << PageGlobalDirIndexBits::USIZE) - 1)
    << PageIndexBits::USIZE
        + PageTableIndexBits::USIZE
        + PageDirIndexBits::USIZE
        + PageUpperDirIndexBits::USIZE);

#[derive(Debug)]
pub struct PageGlobalDirectory {}

impl Maps<PageUpperDirectory> for PageGlobalDirectory {
    fn map_granule<RootG, Root>(
        &mut self,
        upper_dir: &LocalCap<PageUpperDirectory>,
        addr: usize,
        root: &mut LocalCap<Root>,
        _rights: CapRights,
    ) -> Result<(), MappingError>
    where
        Root: Maps<RootG>,
        Root: CapType,
        RootG: CapType,
    {
        match unsafe {
            seL4_ARM_PageUpperDirectory_Map(
                upper_dir.cptr,
                root.cptr,
                addr & GD_MASK,
                seL4_ARM_VMAttributes_seL4_ARM_PageCacheable
                    | seL4_ARM_VMAttributes_seL4_ARM_ParityEnabled,
            )
        } {
            0 => Ok(()),
            e => Err(MappingError::IntermediateLayerFailure(
                SeL4Error::PageUpperDirectoryMap(e),
            )),
        }
    }
}

impl CapType for PageGlobalDirectory {}
impl PhantomCap for PageGlobalDirectory {
    fn phantom_instance() -> Self {
        PageGlobalDirectory {}
    }
}

impl DirectRetype for PageGlobalDirectory {
    type SizeBits = super::super::PageGlobalDirBits;
    fn sel4_type_id() -> usize {
        _mode_object_seL4_ARM_PageGlobalDirectoryObject as usize
    }
}
