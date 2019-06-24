use core::marker::PhantomData;

use selfe_sys::*;

use crate::arch::LargePageBits;

use crate::cap::{
    memory_kind, role, CNodeRole, Cap, CapType, CopyAliasable, DirectRetype, MemoryKind, PhantomCap,
};

#[derive(Debug)]
pub struct UnmappedLargePage<Kind: MemoryKind> {
    _kind: PhantomData<Kind>,
}

impl<Kind: MemoryKind> CapType for UnmappedLargePage<Kind> {}

impl<Kind: MemoryKind> PhantomCap for UnmappedLargePage<Kind> {
    fn phantom_instance() -> Self {
        UnmappedLargePage {
            _kind: PhantomData::<Kind>,
        }
    }
}

impl DirectRetype for UnmappedLargePage<memory_kind::General> {
    type SizeBits = LargePageBits;
    fn sel4_type_id() -> usize {
        _object_seL4_ARM_LargePageObject as usize
    }
}

impl<Kind: MemoryKind> CopyAliasable for UnmappedLargePage<Kind> {
    type CopyOutput = Self;
}

#[derive(Debug)]
pub struct MappedLargePage<Role: CNodeRole, Kind: MemoryKind> {
    pub(crate) vaddr: usize,
    pub(crate) _role: PhantomData<Role>,
    pub(crate) _kind: PhantomData<Kind>,
}

impl Cap<MappedLargePage<role::Child, memory_kind::Device>, role::Local> {
    /// `vaddr` allows a parent process to extract the vaddr of a
    /// device page mapped into a child's VSpace.
    pub fn vaddr(&self) -> usize {
        self.cap_data.vaddr
    }
}

impl<Role: CNodeRole, Kind: MemoryKind> CapType for MappedLargePage<Role, Kind> {}

impl<Role: CNodeRole, Kind: MemoryKind> CopyAliasable for MappedLargePage<Role, Kind> {
    type CopyOutput = UnmappedLargePage<Kind>;
}
