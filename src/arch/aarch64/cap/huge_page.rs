use core::marker::PhantomData;

use selfe_sys::*;

use crate::arch::HugePageBits;

use crate::cap::{
    memory_kind, page_state, role, CNodeRole, Cap, CapType, CopyAliasable, DirectRetype,
    MemoryKind, PageState, PhantomCap,
};

#[derive(Debug, Clone)]
pub struct HugePage<State: PageState, Kind: MemoryKind> {
    pub(crate) state: State,
    pub(crate) memory_kind: Kind,
}

impl<State: PageState, Kind: MemoryKind> CapType for HugePage<State, Kind> {}

impl DirectRetype for HugePage<page_state::Unmapped, memory_kind::General> {
    type SizeBits = HugePageBits;
    fn sel4_type_id() -> usize {
        _mode_object_seL4_ARM_HugePageObject as usize
    }
}

impl<Kind: MemoryKind> CopyAliasable for HugePage<page_state::Unmapped, Kind> {
    type CopyOutput = Self;
}

impl<Kind: MemoryKind> CopyAliasable for HugePage<page_state::Mapped, Kind> {
    type CopyOutput = HugePage<page_state::Unmapped, Kind>;
}

impl<'a, State: PageState, Kind: MemoryKind> From<&'a HugePage<State, Kind>>
    for HugePage<State, Kind>
{
    fn from(val: &'a HugePage<State, Kind>) -> Self {
        val.clone()
    }
}
impl<'a, Kind: MemoryKind> From<&'a HugePage<page_state::Mapped, Kind>>
    for HugePage<page_state::Unmapped, Kind>
{
    fn from(val: &'a HugePage<page_state::Mapped, Kind>) -> Self {
        HugePage {
            state: page_state::Unmapped {},
            memory_kind: val.memory_kind.clone(),
        }
    }
}
