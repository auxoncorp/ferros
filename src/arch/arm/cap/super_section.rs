use selfe_sys::*;
use typenum::U24;

use crate::cap::{
    memory_kind, page_state, CapType, CopyAliasable, DirectRetype, MemoryKind, PageState,
};

#[derive(Debug, Clone)]
pub struct SuperSection<State: PageState, Kind: MemoryKind> {
    pub(crate) state: State,
    pub(crate) memory_kind: Kind,
}

impl<State: PageState, Kind: MemoryKind> CapType for SuperSection<State, Kind> {}

impl DirectRetype for SuperSection<page_state::Unmapped, memory_kind::General> {
    type SizeBits = U24;
    fn sel4_type_id() -> usize {
        _object_seL4_ARM_SuperSectionObject as usize
    }
}

impl<Kind: MemoryKind> CopyAliasable for SuperSection<page_state::Unmapped, Kind> {
    type CopyOutput = Self;
}

impl<Kind: MemoryKind> CopyAliasable for SuperSection<page_state::Mapped, Kind> {
    type CopyOutput = SuperSection<page_state::Unmapped, Kind>;
}

impl<'a, State: PageState, Kind: MemoryKind> From<&'a SuperSection<State, Kind>>
    for SuperSection<State, Kind>
{
    fn from(val: &'a SuperSection<State, Kind>) -> Self {
        val.clone()
    }
}
impl<'a, Kind: MemoryKind> From<&'a SuperSection<page_state::Mapped, Kind>>
    for SuperSection<page_state::Unmapped, Kind>
{
    fn from(val: &'a SuperSection<page_state::Mapped, Kind>) -> Self {
        SuperSection {
            state: page_state::Unmapped {},
            memory_kind: val.memory_kind.clone(),
        }
    }
}
