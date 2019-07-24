use core::marker::PhantomData;

use selfe_sys::*;

use crate::arch::LargePageBits;

use crate::cap::{
    memory_kind, page_state, role, CNodeRole, Cap, CapType, CopyAliasable, DirectRetype,
    MemoryKind, PageState, PhantomCap,
};

#[derive(Debug, Clone)]
pub struct LargePage<State: PageState, Kind: MemoryKind> {
    pub(crate) state: State,
    pub(crate) memory_kind: Kind,
}

impl<State: PageState, Kind: MemoryKind> CapType for LargePage<State, Kind> {}

impl DirectRetype for LargePage<page_state::Unmapped, memory_kind::General> {
    type SizeBits = LargePageBits;
    fn sel4_type_id() -> usize {
        _object_seL4_ARM_LargePageObject as usize
    }
}

impl<Kind: MemoryKind> CopyAliasable for LargePage<page_state::Unmapped, Kind> {
    type CopyOutput = Self;
}

impl<Kind: MemoryKind> CopyAliasable for LargePage<page_state::Mapped, Kind> {
    type CopyOutput = LargePage<page_state::Unmapped, Kind>;
}

impl<'a, State: PageState, Kind: MemoryKind> From<&'a LargePage<State, Kind>>
    for LargePage<State, Kind>
{
    fn from(val: &'a LargePage<State, Kind>) -> Self {
        val.clone()
    }
}
impl<'a, Kind: MemoryKind> From<&'a LargePage<page_state::Mapped, Kind>>
    for LargePage<page_state::Unmapped, Kind>
{
    fn from(val: &'a LargePage<page_state::Mapped, Kind>) -> Self {
        LargePage {
            state: page_state::Unmapped {},
            memory_kind: val.memory_kind.clone(),
        }
    }
}
