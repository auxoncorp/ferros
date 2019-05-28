use core::marker::PhantomData;

use selfe_sys::*;

use typenum::*;

use crate::arch::HugePageBits;

use crate::cap::{
    memory_kind, role, CNodeRole, Cap, CapType, CopyAliasable, DirectRetype, MemoryKind, PhantomCap,
};

#[derive(Debug)]
pub struct UnmappedHugePage<Kind: MemoryKind> {
    _kind: PhantomData<Kind>,
}

impl<Kind: MemoryKind> CapType for UnmappedHugePage<Kind> {}

impl<Kind: MemoryKind> PhantomCap for UnmappedHugePage<Kind> {
    fn phantom_instance() -> Self {
        UnmappedHugePage {
            _kind: PhantomData::<Kind>,
        }
    }
}

impl DirectRetype for UnmappedHugePage<memory_kind::General> {
    type SizeBits = HugePageBits;
    fn sel4_type_id() -> usize {
        _object_seL4_ARM_HugePageObject as usize
    }
}

impl<Kind: MemoryKind> CopyAliasable for UnmappedHugePage<Kind> {
    type CopyOutput = Self;
}

#[derive(Debug)]
pub struct MappedHugePage<Role: CNodeRole, Kind: MemoryKind> {
    pub(crate) vaddr: usize,
    pub(crate) _role: PhantomData<Role>,
    pub(crate) _kind: PhantomData<Kind>,
}

impl Cap<MappedHugePage<role::Child, memory_kind::Device>, role::Local> {
    /// `vaddr` allows a parent process to extract the vaddr of a
    /// device page mapped into a child's VSpace.
    pub fn vaddr(&self) -> usize {
        self.cap_data.vaddr
    }
}

impl<Role: CNodeRole, Kind: MemoryKind> CapType for MappedHugePage<Role, Kind> {}

impl<Role: CNodeRole, Kind: MemoryKind> CopyAliasable for MappedHugePage<Role, Kind> {
    type CopyOutput = UnmappedHugePage<Kind>;
}
