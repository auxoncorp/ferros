// TODO: It's important that AssignedPageDirectory can never be moved or deleted
// (or copied, likely), as that leads to ugly cptr aliasing issues that we're
// not able to detect at compile time. Write compile-tests to ensure that it
// doesn't implement those marker traits.
#[derive(Debug, PartialEq)]
pub struct AssignedPageDirectory<FreeSlots: Unsigned, Role: CNodeRole> {
    pub(super) next_free_slot: usize,
    pub(super) _free_slots: PhantomData<FreeSlots>,
    pub(super) _role: PhantomData<Role>,
}

impl<FreeSlots: Unsigned, Role: CNodeRole> CapType for AssignedPageDirectory<FreeSlots, Role> {}

#[derive(Debug)]
pub struct UnassignedPageDirectory {}

impl CapType for UnassignedPageDirectory {}

impl PhantomCap for UnassignedPageDirectory {
    fn phantom_instance() -> Self {
        Self {}
    }
}

impl DirectRetype for UnassignedPageDirectory {
    type SizeBits = U14;
    fn sel4_type_id() -> usize {
        _object_seL4_ARM_PageDirectoryObject as usize
    }
}

//////////////////////////////
// Paging object: PageTable //
//////////////////////////////

#[derive(Debug)]
pub struct UnmappedPageTable {}

impl CapType for UnmappedPageTable {}

impl PhantomCap for UnmappedPageTable {
    fn phantom_instance() -> Self {
        Self {}
    }
}

impl CopyAliasable for UnmappedPageTable {
    type CopyOutput = Self;
}

impl DirectRetype for UnmappedPageTable {
    type SizeBits = U10;
    fn sel4_type_id() -> usize {
        _object_seL4_ARM_PageTableObject as usize
    }
}

#[derive(Debug)]
pub struct MappedPageTable<FreeSlots: Unsigned, Role: CNodeRole> {
    pub(super) vaddr: usize,
    pub(super) next_free_slot: usize,
    pub(super) _free_slots: PhantomData<FreeSlots>,
    pub(super) _role: PhantomData<Role>,
}

impl<FreeSlots: Unsigned, Role: CNodeRole> CapType for MappedPageTable<FreeSlots, Role> {}

impl<FreeSlots: Unsigned, Role: CNodeRole> CopyAliasable for MappedPageTable<FreeSlots, Role> {
    type CopyOutput = UnmappedPageTable;
}

impl<FreeSlots: Unsigned, Role: CNodeRole> Movable for MappedPageTable<FreeSlots, Role> {}

/////////////////////////
// Paging object: Page //
/////////////////////////

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
    type SizeBits = paging::PageBits;
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

//////////////////////////////
// Paging object: LargePage //
//////////////////////////////

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
    type SizeBits = paging::LargePageBits;
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

//////////////////////////////
// Paging object: Section //
//////////////////////////////

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
    type SizeBits = paging::SectionBits;
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

/////////////////////////////////
// Paging object: SuperSection //
/////////////////////////////////

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
    type SizeBits = paging::SuperSectionBits;
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

impl<Role: CNodeRole> CapType for CNode<Role> {}

impl<Size: Unsigned, Role: CNodeRole> CapType for CNodeSlotsData<Size, Role> {}
