use core::marker::PhantomData;
use core::ops::Sub;
use crate::userland::{CNode, CapRights, SeL4Error};
use sel4_sys::*;
use typenum::operator_aliases::Sub1;
use typenum::{Unsigned, B1};

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
    // TODO - find out where the actual size of the fixed-size objects are specified in seL4-land
    // and pipe them through to the implementations of this trait as an associated type parameter,
    // selected either through `cfg` attributes or reference to `build.rs` generated code that inspects
    // feature flags passed by cargo-fel4.
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
/// TODO - Review all of the CapType structs and apply where necessary
pub trait Delible {}

#[derive(Debug)]
pub struct Cap<CT: CapType, Role: CNodeRole> {
    pub cptr: usize,
    // TODO: put this back to pub(super)
    pub(crate) cap_data: CT,
    pub(crate) _role: PhantomData<Role>,
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
pub struct Untyped<BitSize: Unsigned> {
    _bit_size: PhantomData<BitSize>,
}

impl<BitSize: Unsigned> CapType for Untyped<BitSize> {}

impl<BitSize: Unsigned> PhantomCap for Untyped<BitSize> {
    fn phantom_instance() -> Self {
        Untyped::<BitSize> {
            _bit_size: PhantomData::<BitSize>,
        }
    }
}

impl<BitSize: Unsigned> Movable for Untyped<BitSize> {}
impl<BitSize: Unsigned> Delible for Untyped<BitSize> {}

#[derive(Debug)]
pub struct ThreadControlBlock {}

impl CapType for ThreadControlBlock {}

impl PhantomCap for ThreadControlBlock {
    fn phantom_instance() -> Self {
        Self {}
    }
}

impl DirectRetype for ThreadControlBlock {
    fn sel4_type_id() -> usize {
        api_object_seL4_TCBObject as usize
    }
}

impl CopyAliasable for ThreadControlBlock {
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

pub struct IRQHandle {
    pub(crate) irq: u8,
}

impl CapType for IRQHandle {}

#[derive(Debug)]
pub struct ASIDControl {}

impl CapType for ASIDControl {}

impl PhantomCap for ASIDControl {
    fn phantom_instance() -> Self {
        Self {}
    }
}
#[derive(Debug)]
pub struct ASIDPool<FreeSlots: Unsigned> {
    pub(super) next_free_slot: usize,
    pub(super) _free_slots: PhantomData<FreeSlots>,
}

impl<FreeSlots: Unsigned> CapType for ASIDPool<FreeSlots> {}

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
    fn sel4_type_id() -> usize {
        api_object_seL4_NotificationObject as usize
    }
}

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
    fn sel4_type_id() -> usize {
        _object_seL4_ARM_PageDirectoryObject as usize
    }
}

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

#[derive(Debug)]
pub struct UnmappedPage {}

impl CapType for UnmappedPage {}

impl PhantomCap for UnmappedPage {
    fn phantom_instance() -> Self {
        Self {}
    }
}

impl DirectRetype for UnmappedPage {
    fn sel4_type_id() -> usize {
        _object_seL4_ARM_SmallPageObject as usize
    }
}

impl CopyAliasable for UnmappedPage {
    type CopyOutput = Self;
}

#[derive(Debug)]
pub struct MappedPage<Role: CNodeRole> {
    pub(crate) vaddr: usize,
    pub(crate) _role: PhantomData<Role>,
}

impl<Role: CNodeRole> CapType for MappedPage<Role> {}

impl<Role: CNodeRole> CopyAliasable for MappedPage<Role> {
    type CopyOutput = UnmappedPage;
}

impl<FreeSlots: typenum::Unsigned, Role: CNodeRole> CapType for CNode<FreeSlots, Role> {}

mod private {
    use super::{role, CNodeRole, Unsigned};

    pub trait SealedRole {}
    impl SealedRole for role::Local {}
    impl SealedRole for role::Child {}

    pub trait SealedCapType {}
    impl<BitSize: typenum::Unsigned> SealedCapType for super::Untyped<BitSize> {}
    impl<FreeSlots: typenum::Unsigned, Role: CNodeRole> SealedCapType
        for super::CNode<FreeSlots, Role>
    {
    }
    impl SealedCapType for super::ThreadControlBlock {}
    impl SealedCapType for super::Endpoint {}
    impl SealedCapType for super::Notification {}
    impl SealedCapType for super::ASIDControl {}
    impl<FreeSlots: Unsigned> SealedCapType for super::ASIDPool<FreeSlots> {}
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

    impl SealedCapType for super::UnmappedPage {}
    impl<Role: CNodeRole> SealedCapType for super::MappedPage<Role> {}
    impl SealedCapType for super::IRQControl {}
    impl SealedCapType for super::IRQHandle {}
}

impl<CT: CapType> Cap<CT, role::Local> {
    pub fn copy<SourceFreeSlots: Unsigned, FreeSlots: Unsigned, DestRole: CNodeRole>(
        &self,
        src_cnode: &LocalCap<CNode<SourceFreeSlots, role::Local>>,
        dest_cnode: LocalCap<CNode<FreeSlots, DestRole>>,
        rights: CapRights,
    ) -> Result<
        (
            Cap<CT::CopyOutput, DestRole>,
            LocalCap<CNode<Sub1<FreeSlots>, DestRole>>,
        ),
        SeL4Error,
    >
    where
        FreeSlots: Sub<B1>,
        Sub1<FreeSlots>: Unsigned,
        CT: CopyAliasable,
        <CT as CopyAliasable>::CopyOutput: PhantomCap,
    {
        let (dest_cnode, dest_slot) = dest_cnode.consume_slot();

        let err = unsafe {
            seL4_CNode_Copy(
                dest_slot.cptr,      // _service
                dest_slot.offset,    // index
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
            Ok((
                Cap {
                    cptr: dest_slot.offset,
                    cap_data: PhantomCap::phantom_instance(),
                    _role: PhantomData,
                },
                dest_cnode,
            ))
        }
    }

    pub fn copy_inside_cnode<FreeSlots: Unsigned>(
        &self,
        src_and_dest_cnode: LocalCap<CNode<FreeSlots, role::Local>>,
        rights: CapRights,
    ) -> Result<
        (
            LocalCap<CT::CopyOutput>,
            LocalCap<CNode<Sub1<FreeSlots>, role::Local>>,
        ),
        SeL4Error,
    >
    where
        FreeSlots: Sub<B1>,
        Sub1<FreeSlots>: Unsigned,
        CT: CopyAliasable,
        <CT as CopyAliasable>::CopyOutput: PhantomCap,
    {
        let (src_and_dest_cnode, dest_slot) = src_and_dest_cnode.consume_slot();

        let err = unsafe {
            seL4_CNode_Copy(
                dest_slot.cptr,      // _service
                dest_slot.offset,    // index
                seL4_WordBits as u8, // depth
                // Since src_cnode is restricted to Root, the cptr must
                // actually be the slot index
                src_and_dest_cnode.cptr, // src_root
                self.cptr,               // src_index
                seL4_WordBits as u8,     // src_depth
                rights.into(),           // rights
            )
        };

        if err != 0 {
            Err(SeL4Error::CNodeCopy(err))
        } else {
            Ok((
                Cap {
                    cptr: dest_slot.offset,
                    cap_data: PhantomCap::phantom_instance(),
                    _role: PhantomData,
                },
                src_and_dest_cnode,
            ))
        }
    }

    /// Copy a capability while also setting rights and a badge
    pub(crate) fn mint<SourceFreeSlots: Unsigned, FreeSlots: Unsigned, DestRole: CNodeRole>(
        &self,
        src_cnode: &LocalCap<CNode<SourceFreeSlots, role::Local>>,
        dest_cnode: LocalCap<CNode<FreeSlots, DestRole>>,
        rights: CapRights,
        badge: Badge,
    ) -> Result<
        (
            Cap<CT::CopyOutput, DestRole>,
            LocalCap<CNode<Sub1<FreeSlots>, DestRole>>,
        ),
        SeL4Error,
    >
    where
        FreeSlots: Sub<B1>,
        Sub1<FreeSlots>: Unsigned,
        CT: Mintable,
        CT: CopyAliasable,
        CT: PhantomCap,
        <CT as CopyAliasable>::CopyOutput: PhantomCap,
    {
        let (dest_cnode, dest_slot) = dest_cnode.consume_slot();

        let err = unsafe {
            seL4_CNode_Mint(
                dest_slot.cptr,      // _service
                dest_slot.offset,    // dest index
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
            Ok((
                Cap {
                    cptr: dest_slot.offset,
                    cap_data: PhantomCap::phantom_instance(),
                    _role: PhantomData,
                },
                dest_cnode,
            ))
        }
    }

    /// Migrate a capability from one CNode to another.
    pub fn move_to_cnode<SourceFreeSlots: Unsigned, FreeSlots: Unsigned, DestRole: CNodeRole>(
        self,
        src_cnode: &LocalCap<CNode<SourceFreeSlots, role::Local>>,
        dest_cnode: LocalCap<CNode<FreeSlots, DestRole>>,
    ) -> Result<
        (
            Cap<CT, DestRole>,
            LocalCap<CNode<Sub1<FreeSlots>, DestRole>>,
        ),
        SeL4Error,
    >
    where
        FreeSlots: Sub<B1>,
        Sub1<FreeSlots>: Unsigned,
        CT: Movable,
    {
        let (dest_cnode, dest_slot) = dest_cnode.consume_slot();

        let err = unsafe {
            seL4_CNode_Move(
                dest_slot.cptr,      // _service
                dest_slot.offset,    // index
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
            Ok((
                Cap {
                    cptr: dest_slot.offset,
                    cap_data: self.cap_data,
                    _role: PhantomData,
                },
                dest_cnode,
            ))
        }
    }

    /// Delete a capability
    pub fn delete<FreeSlots: Unsigned>(
        self,
        parent_cnode: &LocalCap<CNode<FreeSlots, role::Local>>,
    ) -> Result<(), SeL4Error>
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