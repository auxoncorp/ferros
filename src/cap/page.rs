use crate::arch::PageBytes;
use crate::cap::{
    CNodeRole, Cap, CapRangeDataReconstruction, CapType, CopyAliasable, InternalASID, Movable,
};
use typenum::Unsigned;

#[derive(Clone, Debug)]
pub struct Page<State: PageState> {
    pub(crate) state: State,
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
    pub(crate) fn asid(&self) -> InternalASID {
        self.cap_data.state.asid
    }
}
