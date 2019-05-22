use core::marker::PhantomData;

use typenum::*;

use selfe_sys::*;

use crate::cap::{
    memory_kind, role, CNodeRole, Cap, CapType, CopyAliasable, DirectRetype, MemoryKind, PhantomCap,
};

#[derive(Debug)]
pub struct UnmappedPage<Kind: MemoryKind> {
    _kind: PhantomData<Kind>,
}

impl<Kind: MemoryKind> CapType for UnmappedPage<Kind> {}

impl<Kind: MemoryKind> PhantomCap for UnmappedPage<Kind> {
    fn phantom_instance() -> Self {
        UnmappedPage {
            _kind: PhantomData::<Kind>,
        }
    }
}

impl DirectRetype for UnmappedPage<memory_kind::General> {
    type SizeBits = U12;
    fn sel4_type_id() -> usize {
        _object_seL4_ARM_SmallPageObject as usize
    }
}

impl<Kind: MemoryKind> CopyAliasable for UnmappedPage<Kind> {
    type CopyOutput = Self;
}

#[derive(Debug)]
pub struct MappedPage<Role: CNodeRole, Kind: MemoryKind> {
    pub(crate) vaddr: usize,
    pub(crate) _role: PhantomData<Role>,
    pub(crate) _kind: PhantomData<Kind>,
}

impl Cap<MappedPage<role::Child, memory_kind::Device>, role::Local> {
    /// `vaddr` allows a parent process to extract the vaddr of a
    /// device page mapped into a child's VSpace.
    pub fn vaddr(&self) -> usize {
        self.cap_data.vaddr
    }
}

impl<Role: CNodeRole, Kind: MemoryKind> CapType for MappedPage<Role, Kind> {}

impl<Role: CNodeRole, Kind: MemoryKind> CopyAliasable for MappedPage<Role, Kind> {
    type CopyOutput = UnmappedPage<Kind>;
}
