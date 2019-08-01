use crate::arch::PageBytes;
use crate::cap::{
    CNodeRole, Cap, CapRangeDataReconstruction, CapType, CopyAliasable, InternalASID, Movable,
};
use typenum::Unsigned;

#[derive(Clone, Debug)]
pub struct Page<State: PageState> {
    pub(crate) state: State,
}

pub trait PageState:
    private::SealedPageState + Copy + Clone + core::fmt::Debug + Sized + PartialEq
{
    fn offset_by(&self, bytes: usize) -> Option<Self>;
}

pub mod page_state {
    use super::*;

    #[derive(Clone, Copy, Debug, PartialEq)]
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

    #[derive(Clone, Copy, Debug, PartialEq)]
    pub struct Unmapped;
    impl super::PageState for Unmapped {
        fn offset_by(&self, _bytes: usize) -> Option<Self> {
            Some(Unmapped)
        }
    }
}
impl<State: PageState> CapType for Page<State> {}

impl<State: PageState> CopyAliasable for Page<State> {
    type CopyOutput = Page<page_state::Unmapped>;
}
impl<State: PageState> Movable for Page<State> {}

impl<'a, State: PageState> From<&'a Page<State>> for Page<page_state::Unmapped> {
    fn from(_val: &'a Page<State>) -> Self {
        Page {
            state: page_state::Unmapped {},
        }
    }
}

impl<State: PageState> CapRangeDataReconstruction for Page<State> {
    fn reconstruct(index: usize, seed_cap_data: &Self) -> Self {
        Page {
            state: seed_cap_data
                .state
                .offset_by(index * PageBytes::USIZE)
                // TODO - consider making reconstruct fallible
                .expect("Earlier checks confirm the memory fits into available space"),
        }
    }
}

impl<CapRole: CNodeRole> Cap<Page<page_state::Mapped>, CapRole> {
    pub fn vaddr(&self) -> usize {
        self.cap_data.state.vaddr
    }
}

mod private {
    pub trait SealedPageState {}
    impl SealedPageState for super::page_state::Unmapped {}
    impl SealedPageState for super::page_state::Mapped {}
}
