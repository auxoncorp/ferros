use crate::arch::PageBytes;
use crate::cap::{
    granule_state, CNodeRole, Cap, CapRangeDataReconstruction, CapType, CopyAliasable,
    GranuleState, Movable,
};
use typenum::Unsigned;

#[derive(Clone, Debug)]
pub struct Page<State: GranuleState> {
    pub(crate) state: State,
}

impl<State: GranuleState> CapType for Page<State> {}

impl<State: GranuleState> CopyAliasable for Page<State> {
    type CopyOutput = Page<granule_state::Unmapped>;
}

impl<State: GranuleState> Movable for Page<State> {}

impl<'a, State: GranuleState> From<&'a Page<State>> for Page<granule_state::Unmapped> {
    fn from(_val: &'a Page<State>) -> Self {
        Page {
            state: granule_state::Unmapped {},
        }
    }
}

impl<State: GranuleState> CapRangeDataReconstruction for Page<State> {
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

impl<CapRole: CNodeRole> Cap<Page<granule_state::Mapped>, CapRole> {
    pub fn vaddr(&self) -> usize {
        self.cap_data.state.vaddr
    }
}
