use selfe_sys::*;
use typenum::U20;

use crate::cap::{
    memory_kind, page_state, CapType, CopyAliasable, DirectRetype, MemoryKind, PageState,
};

#[derive(Debug, Clone)]
pub struct Section<State: PageState, Kind: MemoryKind> {
    pub(crate) state: State,
    pub(crate) memory_kind: Kind,
}

impl<State: PageState, Kind: MemoryKind> CapType for Section<State, Kind> {}

impl DirectRetype for Section<page_state::Unmapped, memory_kind::General> {
    type SizeBits = U20;
    fn sel4_type_id() -> usize {
        _object_seL4_ARM_SectionObject as usize
    }
}

impl<Kind: MemoryKind> CopyAliasable for Section<page_state::Unmapped, Kind> {
    type CopyOutput = Self;
}

impl<Kind: MemoryKind> CopyAliasable for Section<page_state::Mapped, Kind> {
    type CopyOutput = Section<page_state::Unmapped, Kind>;
}

impl<'a, State: PageState, Kind: MemoryKind> From<&'a Section<State, Kind>>
    for Section<State, Kind>
{
    fn from(val: &'a Section<State, Kind>) -> Self {
        val.clone()
    }
}
impl<'a, Kind: MemoryKind> From<&'a Section<page_state::Mapped, Kind>>
    for Section<page_state::Unmapped, Kind>
{
    fn from(val: &'a Section<page_state::Mapped, Kind>) -> Self {
        Section {
            state: page_state::Unmapped {},
            memory_kind: val.memory_kind.clone(),
        }
    }
}
