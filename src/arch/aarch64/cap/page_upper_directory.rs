use selfe_sys::*;

use typenum::Unsigned;

use crate::cap::{CapType, DirectRetype, LocalCap, PhantomCap};
use crate::error::{ErrorExt, KernelError, SeL4Error};
use crate::userland::CapRights;
use crate::vspace::{MappingError, Maps};

use super::super::{PageDirIndexBits, PageIndexBits, PageTableIndexBits, PageUpperDirIndexBits};
use super::PageDirectory;

const UD_MASK: usize = (((1 << PageUpperDirIndexBits::USIZE) - 1)
    << PageIndexBits::USIZE + PageTableIndexBits::USIZE + PageDirIndexBits::USIZE);

#[derive(Debug)]
pub struct PageUpperDirectory {}

impl Maps<PageDirectory> for PageUpperDirectory {
    fn map_granule<RootLowerLevel, Root>(
        &mut self,
        dir: &LocalCap<PageDirectory>,
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
        match unsafe {
            seL4_ARM_PageDirectory_Map(dir.cptr, root.cptr, addr & UD_MASK, vm_attributes)
        }
        .as_result()
        {
            Ok(_) => Ok(()),
            Err(KernelError::FailedLookup) => Err(MappingError::Overflow),
            Err(e) => Err(MappingError::IntermediateLayerFailure(
                SeL4Error::PageDirectoryMap(e),
            )),
        }
    }
}

impl CapType for PageUpperDirectory {}

impl PhantomCap for PageUpperDirectory {
    fn phantom_instance() -> Self {
        PageUpperDirectory {}
    }
}

impl DirectRetype for PageUpperDirectory {
    type SizeBits = super::super::PageUpperDirBits;
    fn sel4_type_id() -> usize {
        _mode_object_seL4_ARM_PageUpperDirectoryObject as usize
    }
}
