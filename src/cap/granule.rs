use typenum::U0;

use crate::cap::{CapType, DirectRetype, LocalCap};

/// The type returned by the architecture specific implementations of
/// `determine_best_granule_fit`.
pub(crate) struct GranuleInfo {
    /// This granule's size in bits (radix in seL4 parlance).
    size_bits: u8,
    /// How many of them do I need to do this?
    count: u16,
}

/// An abstract way of thinking about the leaves in paging structures
/// across architectures. A Granule can be a Page, a LargePage, a
/// Section, &c.
pub struct Granule<State: GranuleState> {
    /// The size of this granule in bits.
    size_bits: u8,
    /// The seL4 object id.
    type_id: usize,
    /// Is this granule mapped or unmapped and the state that goes
    /// along with that.
    state: State,
}

pub trait GranuleState:
    private::SealedGranuleState + Copy + Clone + core::fmt::Debug + Sized + PartialEq
{
}

pub mod granule_state {
    use crate::cap::asid_pool::InternalASID;

    #[derive(Clone, Copy, Debug, PartialEq)]
    pub struct Mapped {
        pub(crate) vaddr: usize,
        pub(crate) asid: InternalASID,
    }
    impl super::GranuleState for Mapped {}

    #[derive(Clone, Copy, Debug, PartialEq)]
    pub struct Unmapped;
    impl super::GranuleState for Unmapped {}
}

impl<State: GranuleState> CapType for Granule<State> {}

impl DirectRetype for LocalCap<Granule<granule_state::Unmapped>> {
    // `SizeBits` is unused for a Granule; it has a custom
    // implementation of `size_bits`.
    type SizeBits = U0;

    // Same for `sel4_type_id`. It is not used in Granule's case,
    // granule implements `type_id`.
    fn sel4_type_id() -> usize {
        usize::MAX
    }

    fn type_id(&self) -> usize {
        self.type_id
    }

    fn size_bits(&self) -> usize {
        self.size_bits
    }
}

mod private {
    pub trait SealedGranuleState {}
    impl SealedGranuleState for super::granule_state::Unmapped {}
    impl SealedGranuleState for super::granule_state::Mapped {}
}
