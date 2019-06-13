use selfe_sys::*;

use crate::cap::{CapType, CopyAliasable, DirectRetype, LocalCap, PhantomCap};

pub trait PageState: core::fmt::Debug + private::SealedPageState {}

pub mod page_state {
    #[derive(Debug)]
    pub struct Mapped {
        pub(crate) vaddr: usize,
        pub(crate) asid: u32,
    }
    impl super::PageState for Mapped {}

    #[derive(Debug)]
    pub struct Unmapped;
    impl super::PageState for Unmapped {}
}

// TODO - cleanup debug??
#[derive(Debug)]
pub struct Page<State: PageState> {
    pub(crate) state: State,
}

impl LocalCap<Page<page_state::Mapped>> {
    pub fn vaddr(&self) -> usize {
        self.cap_data.state.vaddr
    }
    pub fn asid(&self) -> u32 {
        self.cap_data.state.asid
    }
}

impl<State: PageState> CapType for Page<State> {}

impl DirectRetype for Page<page_state::Unmapped> {
    type SizeBits = super::super::PageBits;
    fn sel4_type_id() -> usize {
        _object_seL4_ARM_SmallPageObject as usize
    }
}

impl CopyAliasable for Page<page_state::Unmapped> {
    type CopyOutput = Self;
}

impl CopyAliasable for Page<page_state::Mapped> {
    type CopyOutput = Page<page_state::Unmapped>;
}

impl PhantomCap for Page<page_state::Unmapped> {
    fn phantom_instance() -> Self {
        Page {
            state: page_state::Unmapped {},
        }
    }
}

mod private {
    pub trait SealedPageState {}
    impl SealedPageState for super::page_state::Unmapped {}
    impl SealedPageState for super::page_state::Mapped {}
}
