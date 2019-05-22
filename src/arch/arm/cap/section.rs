use core::marker::PhantomData;

use typenum::*;

use selfe_sys::*;

use crate::cap::{
    memory_kind, CNodeRole, CapType, CopyAliasable, DirectRetype, LocalCap, MemoryKind, PhantomCap,
};

#[derive(Debug)]
pub struct UnmappedSection<Kind: MemoryKind> {
    _kind: PhantomData<Kind>,
}

impl<Kind: MemoryKind> CapType for UnmappedSection<Kind> {}

impl<Kind: MemoryKind> PhantomCap for UnmappedSection<Kind> {
    fn phantom_instance() -> Self {
        UnmappedSection {
            _kind: PhantomData::<Kind>,
        }
    }
}

impl DirectRetype for UnmappedSection<memory_kind::General> {
    type SizeBits = U20;
    fn sel4_type_id() -> usize {
        _object_seL4_ARM_SectionObject as usize
    }
}

impl<Kind: MemoryKind> CopyAliasable for UnmappedSection<Kind> {
    type CopyOutput = Self;
}

#[derive(Debug)]
pub struct MappedSection<Role: CNodeRole, Kind: MemoryKind> {
    pub(crate) vaddr: usize,
    pub(crate) _role: PhantomData<Role>,
    pub(crate) _kind: PhantomData<Kind>,
}

impl<Role: CNodeRole, Kind: MemoryKind> LocalCap<MappedSection<Role, Kind>> {
    pub fn vaddr(&self) -> usize {
        self.cap_data.vaddr
    }
}

impl<Role: CNodeRole, Kind: MemoryKind> CapType for MappedSection<Role, Kind> {}

impl<Role: CNodeRole, Kind: MemoryKind> CopyAliasable for MappedSection<Role, Kind> {
    type CopyOutput = UnmappedSection<Kind>;
}
