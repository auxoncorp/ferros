use selfe_sys::*;

use crate::cap::{
    memory_kind, CapType, CopyAliasable, DirectRetype, LocalCap, MemoryKind, Movable, PhantomCap,
};
use crate::error::{ErrorExt, SeL4Error};
use core::marker::PhantomData;

pub trait PageState: private::SealedPageState {}

pub mod page_state {
    pub struct Mapped {
        pub(crate) vaddr: usize,
        pub(crate) asid: u32,
    }
    impl super::PageState for Mapped {}

    pub struct Unmapped;
    impl super::PageState for Unmapped {}
}

pub struct Page<State: PageState, Kind: MemoryKind = memory_kind::General> {
    pub(crate) state: State,
    pub(crate) _kind: PhantomData<Kind>,
}

impl<Kind: MemoryKind> LocalCap<Page<page_state::Mapped, Kind>> {
    pub fn vaddr(&self) -> usize {
        self.cap_data.state.vaddr
    }
    pub fn asid(&self) -> u32 {
        self.cap_data.state.asid
    }

    /// Keeping this non-public in order to restrict mapping operations to owners
    /// of a VSpace-related object
    pub(crate) fn unmap(self) -> Result<LocalCap<Page<page_state::Unmapped, Kind>>, SeL4Error> {
        match unsafe { seL4_ARM_Page_Unmap(self.cptr) }.as_result() {
            Ok(_) => Ok(crate::cap::Cap {
                cptr: self.cptr,
                cap_data: Page {
                    state: page_state::Unmapped {},
                    _kind: PhantomData,
                },
                _role: core::marker::PhantomData,
            }),
            Err(e) => Err(SeL4Error::PageUnmap(e)),
        }
    }
}

impl<State: PageState, Kind: MemoryKind> CapType for Page<State, Kind> {}
impl<State: PageState, Kind: MemoryKind> Movable for Page<State, Kind> {}

impl DirectRetype for Page<page_state::Unmapped, memory_kind::General> {
    type SizeBits = super::super::PageBits;
    fn sel4_type_id() -> usize {
        _object_seL4_ARM_SmallPageObject as usize
    }
}

impl<Kind: MemoryKind> CopyAliasable for Page<page_state::Unmapped, Kind> {
    type CopyOutput = Self;
}

impl<Kind: MemoryKind> CopyAliasable for Page<page_state::Mapped, Kind> {
    type CopyOutput = Page<page_state::Unmapped, Kind>;
}

impl<Kind: MemoryKind> PhantomCap for Page<page_state::Unmapped, Kind> {
    fn phantom_instance() -> Self {
        Page {
            state: page_state::Unmapped {},
            _kind: PhantomData,
        }
    }
}

mod private {
    pub trait SealedPageState {}
    impl SealedPageState for super::page_state::Unmapped {}
    impl SealedPageState for super::page_state::Mapped {}
}
