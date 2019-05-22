#[derive(Debug)]
pub struct UnmappedSuperSection<Kind: MemoryKind> {
    _kind: PhantomData<Kind>,
}

impl<Kind: MemoryKind> CapType for UnmappedSuperSection<Kind> {}

impl<Kind: MemoryKind> PhantomCap for UnmappedSuperSection<Kind> {
    fn phantom_instance() -> Self {
        UnmappedSuperSection {
            _kind: PhantomData::<Kind>,
        }
    }
}

impl DirectRetype for UnmappedSuperSection<memory_kind::General> {
    type SizeBits = U24;
    fn sel4_type_id() -> usize {
        _object_seL4_ARM_SuperSectionObject as usize
    }
}

impl<Kind: MemoryKind> CopyAliasable for UnmappedSuperSection<Kind> {
    type CopyOutput = Self;
}

#[derive(Debug)]
pub struct MappedSuperSection<Role: CNodeRole, Kind: MemoryKind> {
    pub(crate) vaddr: usize,
    pub(crate) _role: PhantomData<Role>,
    pub(crate) _kind: PhantomData<Kind>,
}

impl Cap<MappedSuperSection<role::Child, memory_kind::Device>, role::Local> {
    /// `vaddr` allows a parent process to extract the vaddr of a
    /// device page mapped into a child's VSpace.
    pub fn vaddr(&self) -> usize {
        self.cap_data.vaddr
    }
}

impl<Role: CNodeRole, Kind: MemoryKind> CapType for MappedSuperSection<Role, Kind> {}

impl<Role: CNodeRole, Kind: MemoryKind> CopyAliasable for MappedSuperSection<Role, Kind> {
    type CopyOutput = UnmappedSuperSection<Kind>;
}
