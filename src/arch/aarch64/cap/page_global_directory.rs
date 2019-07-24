use selfe_sys::*;

use typenum::Unsigned;

use crate::cap::{CapType, DirectRetype, LocalCap, Movable, PhantomCap};
use crate::error::{ErrorExt, SeL4Error};
use crate::userland::CapRights;
use crate::vspace::{MappingError, Maps};

use super::super::{
    PageDirIndexBits, PageGlobalDirIndexBits, PageIndexBits, PageTableIndexBits,
    PageUpperDirIndexBits,
};
use super::PageUpperDirectory;

const GD_MASK: usize =
    !((1 << PageIndexBits::USIZE + PageTableIndexBits::USIZE + PageDirIndexBits::USIZE) - 1);

#[derive(Debug)]
pub struct PageGlobalDirectory {}

impl Maps<PageUpperDirectory> for PageGlobalDirectory {
    fn map_granule<RootLowerLevel, Root>(
        &self,
        upper_dir: &LocalCap<PageUpperDirectory>,
        addr: usize,
        root: &mut LocalCap<Root>,
        _rights: CapRights,
        vm_attributes: seL4_ARM_VMAttributes,
    ) -> Result<(), MappingError>
    where
        Root: Maps<RootLowerLevel>,
        Root: CapType,
        RootLowerLevel: CapType,
    {
        unsafe {
            seL4_ARM_PageUpperDirectory_Map(
                upper_dir.cptr,
                root.cptr,
                addr & GD_MASK,
                vm_attributes,
            )
        }
        .as_result()
        .map_err(|e| MappingError::IntermediateLayerFailure(SeL4Error::PageUpperDirectoryMap(e)))
    }
}

impl CapType for PageGlobalDirectory {}
impl Movable for PageGlobalDirectory {}
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
