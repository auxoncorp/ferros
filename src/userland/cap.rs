use crate::arch::paging;
use crate::userland::{
    ASIDControl, ASIDPool, AssignedASID, CNode, CNodeSlot, CNodeSlotsData, CapRights, LocalCNode,
    LocalCNodeSlot, SeL4Error, UnassignedASID,
};
use core::marker::PhantomData;
use selfe_sys::*;
use typenum::*;

/// Type-level enum indicating the relative location / Capability Pointer addressing
/// scheme that should be used for the objects parameterized by it.
pub trait CNodeRole: private::SealedRole {}

pub mod role {
    use super::CNodeRole;

    #[derive(Debug, PartialEq)]
    pub struct Local {}
    impl CNodeRole for Local {}

    #[derive(Debug, PartialEq)]
    pub struct Child {}
    impl CNodeRole for Child {}
}

/// Marker trait for CapType implementing structs to indicate that
/// this type of capability can be generated directly
/// from retyping an Untyped
pub trait DirectRetype {
    type SizeBits: Unsigned;
    // TODO - find out where the actual size of the fixed-size objects are specified in seL4-land
    // and pipe them through to the implementations of this trait as an associated type parameter,
    // selected either through `cfg` attributes or reference to `build.rs` generated code that inspects
    // feature flags.
    //type SizeBits: Unsigned;
    fn sel4_type_id() -> usize;
}

/// Marker trait for CapType implementing structs to indicate that
/// instances of this type of capability can be copied and aliased safely
/// when done through the use of this API
pub trait CopyAliasable {
    type CopyOutput: CapType;
}

/// Marker trait for CapType implementing structs to indicate that
/// instances of this type of capability can be copied and aliased safely
/// when done through the use of this API, and furthermore can be
/// granted badges
pub trait Mintable: CopyAliasable {}

/// Internal marker trait for CapType implementing structs that can
/// have meaningful instances created for them purely from
/// their type signatures.
/// TODO - the structures that implement this should be zero-sized,
/// and we ought to enforce that constraint at compile time.
pub trait PhantomCap: Sized {
    fn phantom_instance() -> Self;
}

/// Marker trait for CapType implementing structs that can
/// be moved from one location to another.
/// TODO - Review all of the CapType structs and apply where necessary
pub trait Movable {}

/// Marker trait for CapType implementing structs that can
/// be deleted.
/// TODO - Delible is presently not used for anything important, and represents
/// a risk of invalidating key immutability assumptions. Consider removing it.
pub trait Delible {}

#[derive(Debug)]
pub struct Cap<CT: CapType, Role: CNodeRole> {
    pub cptr: usize,
    // TODO: put this back to pub(super)
    pub(crate) cap_data: CT,
    pub(super) _role: PhantomData<Role>,
}

pub trait CapType: private::SealedCapType {}

pub type LocalCap<T> = Cap<T, role::Local>;
pub type ChildCap<T> = Cap<T, role::Child>;

impl<CT: CapType, Role: CNodeRole> Cap<CT, Role>
where
    CT: PhantomCap,
{
    // TODO most of this should only happen in the bootstrap adapter
    // TODO - Make even more private!
    pub(crate) fn wrap_cptr(cptr: usize) -> Cap<CT, Role> {
        Cap {
            cptr: cptr,
            cap_data: PhantomCap::phantom_instance(),
            _role: PhantomData,
        }
    }
}

/// Never-to-be-exposed internal wrapper around a capability pointer (cptr)
/// to a capability with the following characteristics:
///   * It cannot be moved out of its slot, ever
///   * The underlying capability kernel object cannot be deleted, ever
///   * Its cptr can serve a purpose without access to any other runtime data about that particular capability
///
/// The point of this reference kind is to allow us to carefully pass around
/// cptrs to kernel objects whose validity will not change even if their
/// local Rust-representing instances are consumed, mutated, or dropped.
///
/// The absurdly long name is an intentional deterrent to the use of this type.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct ImmobileIndelibleInertCapabilityReference<CT: CapType> {
    pub(crate) cptr: usize,
    pub(crate) _cap_type: PhantomData<CT>,
}

impl<CT: CapType> ImmobileIndelibleInertCapabilityReference<CT> {
    pub(crate) unsafe fn get_capability_pointer(&self) -> usize {
        self.cptr
    }
    pub(crate) unsafe fn new(cptr: usize) -> Self {
        ImmobileIndelibleInertCapabilityReference {
            cptr: cptr,
            _cap_type: PhantomData,
        }
    }
}

/// Wrapper for an Endpoint or Notification badge.
/// Note that the kernel will ignore any use of the high 4 bits
#[derive(Clone, Copy, PartialEq, PartialOrd, Eq, Ord, Debug, Hash)]
pub struct Badge {
    pub(crate) inner: usize,
}

impl Badge {
    pub fn are_all_overlapping_bits_set(self, other: Badge) -> bool {
        if self.inner == 0 && other.inner == 0 {
            return true;
        }
        let overlap = self.inner & other.inner;
        overlap != 0
    }
}

impl From<usize> for Badge {
    fn from(u: usize) -> Self {
        let shifted_left = u << 4;
        Badge {
            inner: shifted_left >> 4,
        }
    }
}

impl From<Badge> for usize {
    fn from(b: Badge) -> Self {
        b.inner
    }
}

#[derive(Debug)]
pub struct Untyped<BitSize: Unsigned, Kind: MemoryKind = memory_kind::General> {
    pub(crate) _bit_size: PhantomData<BitSize>,
    pub(crate) _kind: PhantomData<Kind>,
}

impl<BitSize: Unsigned, Kind: MemoryKind> CapType for Untyped<BitSize, Kind> {}

impl<BitSize: Unsigned, Kind: MemoryKind> PhantomCap for Untyped<BitSize, Kind> {
    fn phantom_instance() -> Self {
        Untyped::<BitSize, Kind> {
            _bit_size: PhantomData::<BitSize>,
            _kind: PhantomData::<Kind>,
        }
    }
}

impl<BitSize: Unsigned, Kind: MemoryKind> Movable for Untyped<BitSize, Kind> {}

impl<BitSize: Unsigned, Kind: MemoryKind> Delible for Untyped<BitSize, Kind> {}

pub trait MemoryKind: private::SealedMemoryKind {}

pub mod memory_kind {
    use super::MemoryKind;

    #[derive(Debug, PartialEq)]
    pub struct General;
    impl MemoryKind for General {}

    #[derive(Debug, PartialEq)]
    pub struct Device;
    impl MemoryKind for Device {}
}

#[derive(Debug)]
pub struct ThreadControlBlock {}

impl CapType for ThreadControlBlock {}

impl PhantomCap for ThreadControlBlock {
    fn phantom_instance() -> Self {
        Self {}
    }
}

impl DirectRetype for ThreadControlBlock {
    type SizeBits = U11;
    fn sel4_type_id() -> usize {
        api_object_seL4_TCBObject as usize
    }
}

impl CopyAliasable for ThreadControlBlock {
    type CopyOutput = Self;
}

#[derive(Debug)]
pub struct VirtualCpu {}

impl CapType for VirtualCpu {}

impl PhantomCap for VirtualCpu {
    fn phantom_instance() -> Self {
        Self {}
    }
}

impl DirectRetype for VirtualCpu {
    type SizeBits = U12;
    fn sel4_type_id() -> usize {
        _object_seL4_ARM_VCPUObject as usize
    }
}

impl CopyAliasable for VirtualCpu {
    type CopyOutput = Self;
}

// TODO - consider moving IRQ code allocation tracking to compile-time,
// which may be feasible since we treat IRQControl as a global
// singleton.
// The goal of such tracking is to prevent accidental double-binding to a single IRQ
pub struct IRQControl {
    pub(crate) known_handled: [bool; 256],
}

impl CapType for IRQControl {}

pub struct IRQHandler<IRQ: Unsigned, SetState: IRQSetState>
where
    IRQ: IsLess<U256, Output = True>,
{
    pub(crate) _irq: PhantomData<IRQ>,
    pub(crate) _set_state: PhantomData<SetState>,
}

impl<IRQ: Unsigned, SetState: IRQSetState> CapType for IRQHandler<IRQ, SetState> where
    IRQ: IsLess<U256, Output = True>
{
}

impl<IRQ: Unsigned, SetState: IRQSetState> Movable for IRQHandler<IRQ, SetState> where
    IRQ: IsLess<U256, Output = True>
{
}

/// Whether or not an IRQ Handle has been set to a particular Notification
pub trait IRQSetState: private::SealedIRQSetState {}

pub mod irq_state {
    use super::IRQSetState;

    /// Not set to a Notification
    #[derive(Debug, PartialEq)]
    pub struct Unset;
    impl IRQSetState for Unset {}

    /// Set to a Notification
    #[derive(Debug, PartialEq)]
    pub struct Set;
    impl IRQSetState for Set {}
}

//////////////
// IPC Caps //
//////////////

#[derive(Debug)]
pub struct Endpoint {}

impl CapType for Endpoint {}

impl PhantomCap for Endpoint {
    fn phantom_instance() -> Self {
        Self {}
    }
}

impl CopyAliasable for Endpoint {
    type CopyOutput = Self;
}

impl Mintable for Endpoint {}

impl DirectRetype for Endpoint {
    type SizeBits = U4;
    fn sel4_type_id() -> usize {
        api_object_seL4_EndpointObject as usize
    }
}

#[derive(Debug)]
pub struct Notification {}

impl CapType for Notification {}

impl PhantomCap for Notification {
    fn phantom_instance() -> Self {
        Self {}
    }
}

impl CopyAliasable for Notification {
    type CopyOutput = Self;
}

impl Mintable for Notification {}

impl DirectRetype for Notification {
    type SizeBits = U4;
    fn sel4_type_id() -> usize {
        api_object_seL4_NotificationObject as usize
    }
}

//////////////////////////////////
// Paging object: PageDirectory //
//////////////////////////////////

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
    type SizeBits = U12;
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

mod private {
    use super::{irq_state, memory_kind, role, CNodeRole, IRQSetState, MemoryKind};
    use typenum::{IsLess, True, Unsigned, U256};

    pub trait SealedRole {}
    impl SealedRole for role::Local {}
    impl SealedRole for role::Child {}

    pub trait SealedIRQSetState {}
    impl SealedIRQSetState for irq_state::Unset {}
    impl SealedIRQSetState for irq_state::Set {}

    pub trait SealedMemoryKind {}
    impl SealedMemoryKind for memory_kind::General {}
    impl SealedMemoryKind for memory_kind::Device {}

    pub trait SealedCapType {}
    impl<BitSize: typenum::Unsigned, Kind: MemoryKind> SealedCapType for super::Untyped<BitSize, Kind> {}
    impl<Role: CNodeRole> SealedCapType for super::CNode<Role> {}
    impl<Size: Unsigned, Role: CNodeRole> SealedCapType for super::CNodeSlotsData<Size, Role> {}
    impl SealedCapType for super::ThreadControlBlock {}
    impl SealedCapType for super::VirtualCpu {}
    impl SealedCapType for super::Endpoint {}
    impl SealedCapType for super::Notification {}
    impl<FreePools: Unsigned> SealedCapType for super::ASIDControl<FreePools> {}
    impl<FreeSlots: Unsigned> SealedCapType for super::ASIDPool<FreeSlots> {}
    impl SealedCapType for super::UnassignedASID {}
    impl<ThreadCount: Unsigned> SealedCapType for super::AssignedASID<ThreadCount> {}
    impl<FreeSlots: Unsigned, Role: CNodeRole> SealedCapType
        for super::AssignedPageDirectory<FreeSlots, Role>
    {
    }
    impl SealedCapType for super::UnassignedPageDirectory {}
    impl SealedCapType for super::UnmappedPageTable {}
    impl<FreeSlots: Unsigned, Role: CNodeRole> SealedCapType
        for super::MappedPageTable<FreeSlots, Role>
    {
    }

    impl<Kind: MemoryKind> SealedCapType for super::UnmappedPage<Kind> {}
    impl<Role: CNodeRole, Kind: MemoryKind> SealedCapType for super::MappedPage<Role, Kind> {}

    impl<Kind: MemoryKind> SealedCapType for super::UnmappedLargePage<Kind> {}
    impl<Role: CNodeRole, Kind: MemoryKind> SealedCapType for super::MappedLargePage<Role, Kind> {}

    impl<Kind: MemoryKind> SealedCapType for super::UnmappedSection<Kind> {}
    impl<Role: CNodeRole, Kind: MemoryKind> SealedCapType for super::MappedSection<Role, Kind> {}

    impl<Kind: MemoryKind> SealedCapType for super::UnmappedSuperSection<Kind> {}
    impl<Role: CNodeRole, Kind: MemoryKind> SealedCapType for super::MappedSuperSection<Role, Kind> {}

    impl SealedCapType for super::IRQControl {}
    impl<IRQ: Unsigned, SetState: IRQSetState> SealedCapType for super::IRQHandler<IRQ, SetState> where
        IRQ: IsLess<U256, Output = True>
    {
    }
}

impl<CT: CapType> LocalCap<CT> {
    /// Copy a capability from one CNode to another CNode
    pub fn copy<DestRole: CNodeRole>(
        &self,
        src_cnode: &LocalCap<LocalCNode>,
        dest_slot: CNodeSlot<DestRole>,
        rights: CapRights,
    ) -> Result<Cap<CT::CopyOutput, DestRole>, SeL4Error>
    where
        CT: CopyAliasable,
        <CT as CopyAliasable>::CopyOutput: PhantomCap,
    {
        let (dest_cptr, dest_offset, _) = dest_slot.elim();
        let err = unsafe {
            seL4_CNode_Copy(
                dest_cptr,           // _service
                dest_offset,         // index
                seL4_WordBits as u8, // depth
                // Since src_cnode is restricted to Root, the cptr must
                // actually be the slot index
                src_cnode.cptr,      // src_root
                self.cptr,           // src_index
                seL4_WordBits as u8, // src_depth
                rights.into(),       // rights
            )
        };

        if err != 0 {
            Err(SeL4Error::CNodeCopy(err))
        } else {
            Ok(Cap {
                cptr: dest_offset,
                cap_data: PhantomCap::phantom_instance(),
                _role: PhantomData,
            })
        }
    }

    /// Copy a capability to another CNode while also setting rights and a badge
    pub(crate) fn mint_new<DestRole: CNodeRole>(
        &self,
        src_cnode: &LocalCap<LocalCNode>,
        dest_slot: CNodeSlot<DestRole>,
        rights: CapRights,
        badge: Badge,
    ) -> Result<Cap<CT::CopyOutput, DestRole>, SeL4Error>
    where
        CT: Mintable,
        CT: CopyAliasable,
        CT: PhantomCap,
        <CT as CopyAliasable>::CopyOutput: PhantomCap,
    {
        let (dest_cptr, dest_offset, _) = dest_slot.elim();
        let err = unsafe {
            seL4_CNode_Mint(
                dest_cptr,           // _service
                dest_offset,         // dest index
                seL4_WordBits as u8, // dest depth
                // Since src_cnode is restricted to Root, the cptr must
                // actually be the slot index
                src_cnode.cptr,      // src_root
                self.cptr,           // src_index
                seL4_WordBits as u8, // src_depth
                rights.into(),       // rights
                badge.into(),        // badge
            )
        };

        if err != 0 {
            Err(SeL4Error::CNodeMint(err))
        } else {
            Ok(Cap {
                cptr: dest_offset,
                cap_data: PhantomCap::phantom_instance(),
                _role: PhantomData,
            })
        }
    }

    /// Copy a capability to another CNode while also setting rights and a badge
    pub(crate) fn mint<DestRole: CNodeRole>(
        &self,
        src_cnode: &LocalCap<LocalCNode>,
        dest_slot: CNodeSlot<DestRole>,
        rights: CapRights,
        badge: Badge,
    ) -> Result<Cap<CT::CopyOutput, DestRole>, SeL4Error>
    where
        CT: Mintable,
        CT: CopyAliasable,
        CT: PhantomCap,
        <CT as CopyAliasable>::CopyOutput: PhantomCap,
    {
        let (dest_cptr, dest_offset, _) = dest_slot.elim();
        let err = unsafe {
            seL4_CNode_Mint(
                dest_cptr,           // _service
                dest_offset,         // dest index
                seL4_WordBits as u8, // dest depth
                // Since src_cnode is restricted to Root, the cptr must
                // actually be the slot index
                src_cnode.cptr,      // src_root
                self.cptr,           // src_index
                seL4_WordBits as u8, // src_depth
                rights.into(),       // rights
                badge.into(),        // badge
            )
        };

        if err != 0 {
            Err(SeL4Error::CNodeMint(err))
        } else {
            Ok(Cap {
                cptr: dest_offset,
                cap_data: PhantomCap::phantom_instance(),
                _role: PhantomData,
            })
        }
    }

    /// Copy a capability to another slot inside the same CNode while also setting rights and a badge
    pub(crate) fn mint_inside_cnode(
        &self,
        dest_slot: LocalCNodeSlot,
        rights: CapRights,
        badge: Badge,
    ) -> Result<LocalCap<CT::CopyOutput>, SeL4Error>
    where
        CT: Mintable,
        CT: CopyAliasable,
        <CT as CopyAliasable>::CopyOutput: PhantomCap,
    {
        let (dest_cptr, dest_offset, _) = dest_slot.elim();
        let err = unsafe {
            seL4_CNode_Mint(
                dest_cptr,           // _service
                dest_offset,         // index
                seL4_WordBits as u8, // depth
                // Since src_cnode is restricted to Root, the cptr must
                // actually be the slot index
                dest_cptr,           // src_root
                self.cptr,           // src_index
                seL4_WordBits as u8, // src_depth
                rights.into(),       // rights
                badge.into(),        // badge
            )
        };

        if err != 0 {
            Err(SeL4Error::CNodeMint(err))
        } else {
            Ok(Cap {
                cptr: dest_offset,
                cap_data: PhantomCap::phantom_instance(),
                _role: PhantomData,
            })
        }
    }

    /// Migrate a capability from one CNode slot to another.
    pub fn move_to_slot<DestRole: CNodeRole>(
        self,
        src_cnode: &LocalCap<LocalCNode>,
        dest_slot: CNodeSlot<DestRole>,
    ) -> Result<Cap<CT, DestRole>, SeL4Error>
    where
        CT: Movable,
    {
        let (dest_cptr, dest_offset, _) = dest_slot.elim();
        let err = unsafe {
            seL4_CNode_Move(
                dest_cptr,           // _service
                dest_offset,         // index
                seL4_WordBits as u8, // depth
                // Since src_cnode is restricted to Root, the cptr must
                // actually be the slot index
                src_cnode.cptr,      // src_root
                self.cptr,           // src_index
                seL4_WordBits as u8, // src_depth
            )
        };

        if err != 0 {
            Err(SeL4Error::CNodeMove(err))
        } else {
            Ok(Cap {
                cptr: dest_offset,
                cap_data: self.cap_data,
                _role: PhantomData,
            })
        }
    }

    /// Delete a capability
    pub fn delete(self, parent_cnode: &LocalCap<LocalCNode>) -> Result<(), SeL4Error>
    where
        CT: Delible,
    {
        let err = unsafe {
            seL4_CNode_Delete(
                parent_cnode.cptr,   // _service
                self.cptr,           // index
                seL4_WordBits as u8, // depth
            )
        };
        if err != 0 {
            Err(SeL4Error::CNodeDelete(err))
        } else {
            Ok(())
        }
    }
}
