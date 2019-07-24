use crate::cap::{
    memory_kind, CapType, CopyAliasable, InternalASID, LocalCap, MemoryKind, Movable, PhantomCap,
};

#[derive(Clone, Debug, PartialEq)]
pub struct Page<State: PageState, MemKind: MemoryKind> {
    pub(crate) state: State,
    pub(crate) memory_kind: MemKind,
}

impl<State: PageState, MemKind: MemoryKind> CapType for Page<State, MemKind> {}
impl<State: PageState, MemKind: MemoryKind> Movable for Page<State, MemKind> {}
impl<MemKind: MemoryKind> CopyAliasable for Page<page_state::Unmapped, MemKind> {
    type CopyOutput = Self;
}

impl<MemKind: MemoryKind> CopyAliasable for Page<page_state::Mapped, MemKind> {
    type CopyOutput = Page<page_state::Unmapped, MemKind>;
}

impl PhantomCap for Page<page_state::Unmapped, memory_kind::General> {
    fn phantom_instance() -> Self {
        Page {
            state: page_state::Unmapped {},
            memory_kind: memory_kind::General,
        }
    }
}

pub trait PageState:
    private::SealedPageState + core::fmt::Debug + Clone + Sized + PartialEq
{
    fn offset_by(&self, bytes: usize) -> Option<Self>;
}

pub mod page_state {
    use super::*;

    #[derive(Clone, Debug, PartialEq)]
    pub struct Mapped {
        pub(crate) vaddr: usize,
        pub(crate) asid: InternalASID,
    }
    impl super::PageState for Mapped {
        fn offset_by(&self, bytes: usize) -> Option<Self> {
            if let Some(b) = self.vaddr.checked_add(bytes) {
                Some(Mapped {
                    vaddr: b,
                    asid: self.asid,
                })
            } else {
                None
            }
        }
    }

    #[derive(Clone, Debug, PartialEq)]
    pub struct Unmapped;
    impl super::PageState for Unmapped {
        fn offset_by(&self, _bytes: usize) -> Option<Self> {
            Some(Unmapped)
        }
    }
}

mod private {
    pub trait SealedPageState {}
    impl SealedPageState for super::page_state::Unmapped {}
    impl SealedPageState for super::page_state::Mapped {}
}

impl<MemKind: MemoryKind> LocalCap<Page<page_state::Mapped, MemKind>> {
    pub fn vaddr(&self) -> usize {
        self.cap_data.state.vaddr
    }
    pub(crate) fn asid(&self) -> InternalASID {
        self.cap_data.state.asid
    }
}

impl<'a, State: PageState, Kind: MemoryKind> From<&'a Page<State, Kind>> for Page<State, Kind> {
    fn from(val: &'a Page<State, Kind>) -> Self {
        val.clone()
    }
}
impl<'a, Kind: MemoryKind> From<&'a Page<page_state::Mapped, Kind>>
    for Page<page_state::Unmapped, Kind>
{
    fn from(val: &'a Page<page_state::Mapped, Kind>) -> Self {
        Page {
            state: page_state::Unmapped {},
            memory_kind: val.memory_kind.clone(),
        }
    }
}
