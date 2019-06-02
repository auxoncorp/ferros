use selfe_sys::*;

use typenum::Unsigned;

use crate::cap::{role, Cap, CapType, WCNodeSlots, WUntyped};
use crate::userland::CapRights;
use crate::vspace_too::{MappingError, Maps, PagingTop};

use super::super::{PageIndexBits, PageTableIndexBits};
use super::page_table_too::PageTable;

const PT_MASK: usize = (((1 << PageTableIndexBits::USIZE) - 1) << PageIndexBits::USIZE);

#[derive(Debug)]
pub struct PageDirectory {}

impl Maps<Cap<PageTable, role::Local>> for Cap<PageDirectory, role::Local> {
    fn map_item<RootG, Root>(
        &mut self,
        table: &Cap<PageTable, role::Local>,
        addr: usize,
        root: &mut PagingTop<RootG, Root>,
        _rights: CapRights,
        _ut: &mut WUntyped,
        _slots: &mut WCNodeSlots,
    ) -> Result<(), MappingError>
    where
        Root: Maps<RootG>,
        Root: CapType,
    {
        match unsafe {
            seL4_ARM_PageTable_Map(
                table.cptr,
                addr & PT_MASK,
                root.layer.cptr,
                seL4_ARM_VMAttributes_seL4_ARM_PageCacheable
                    | seL4_ARM_VMAttributes_seL4_ARM_ParityEnabled,
            )
        } {
            0 => Ok(()),
            e => Err(MappingError::IntermediateLayerFailure(e)),
        }
    }
}

impl CapType for PageDirectory {}
