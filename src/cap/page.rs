use crate::arch::PageBytes;
use crate::cap::{
    memory_kind, CapRangeDataReconstruction, CapType, CopyAliasable, InternalASID, LocalCap,
    MemoryKind, Movable, PhantomCap,
};
use typenum::Unsigned;

#[derive(Clone, Debug)]
pub struct Page<State: PageState, MemKind: MemoryKind> {
    pub(crate) state: State,
    pub(crate) kind: MemKind,
}
impl<State: PageState, MemKind: MemoryKind> CapType for Page<State, MemKind> {}

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
            kind: memory_kind::General {},
        }
    }
}
impl<'a, State: PageState, MemKind: MemoryKind> From<&'a Page<State, MemKind>>
    for Page<State, MemKind>
{
    fn from(val: &'a Page<State, MemKind>) -> Self {
        val.clone()
    }
}
impl<'a, MemKind: MemoryKind> From<&'a Page<page_state::Mapped, MemKind>>
    for Page<page_state::Unmapped, MemKind>
{
    fn from(val: &'a Page<page_state::Mapped, MemKind>) -> Self {
        Page {
            state: page_state::Unmapped {},
            kind: val.kind.clone(),
        }
    }
}
impl<State: PageState, MemKind: MemoryKind> CapRangeDataReconstruction for Page<State, MemKind> {
    fn reconstruct(index: usize, seed_cap_data: &Self) -> Self {
        Page {
            state: seed_cap_data
                .state
                .offset_by(index * PageBytes::USIZE)
                // TODO - consider making reconstruct fallible
                .expect("Earlier checks confirm the memory fits into available space"),
            kind: seed_cap_data
                .kind
                .offset_by(index * PageBytes::USIZE)
                // TODO - consider making reconstruct fallible
                .expect("Earlier checks confirm the memory fits into available space"),
        }
    }
}

impl<MemKind: MemoryKind> LocalCap<Page<page_state::Mapped, MemKind>> {
    pub fn vaddr(&self) -> usize {
        self.cap_data.state.vaddr
    }
    pub(crate) fn asid(&self) -> InternalASID {
        self.cap_data.state.asid
    }
}

mod private {
    pub trait SealedPageState {}
    impl SealedPageState for super::page_state::Unmapped {}
    impl SealedPageState for super::page_state::Mapped {}
}
