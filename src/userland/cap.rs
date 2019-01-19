use core::marker::PhantomData;
use core::ops::Sub;
use crate::userland::{CNode, Error, CapRights};
use sel4_sys::*;
use typenum::operator_aliases::Sub1;
use typenum::{Unsigned, B1};

/// Type-level enum indicating the relative location / Capability Pointer addressing
/// scheme that should be used for the objects parameterized by it.
pub trait CNodeRole: private::SealedRole {}

pub mod role {
    use super::CNodeRole;

    #[derive(Debug)]
    pub struct Local {}
    impl CNodeRole for Local {}

    #[derive(Debug)]
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
    pub(super) cap_data: CT,
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
    pub fn wrap_cptr(cptr: usize) -> Cap<CT, Role> {
        Cap {
            cptr: cptr,
            cap_data: PhantomCap::phantom_instance(),
            _role: PhantomData,
        }
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

#[derive(Debug)]
pub struct ASIDControl {}

impl CapType for ASIDControl {}

impl PhantomCap for ASIDControl {
    fn phantom_instance() -> Self {
        Self {}
    }
}

// asid pool
// TODO: track capacity with the types
// TODO: track in the pagedirectory type whether it has been assigned (mapped), and for pagetable too
#[derive(Debug)]
pub struct ASIDPool {}

impl CapType for ASIDPool {}

impl PhantomCap for ASIDPool {
    fn phantom_instance() -> Self {
        Self {}
    }
}

impl CopyAliasable for ASIDPool {
    type CopyOutput = Self;
}

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

impl DirectRetype for Endpoint {
    fn sel4_type_id() -> usize {
        api_object_seL4_EndpointObject as usize
    }
}

#[derive(Debug)]
pub struct AssignedPageDirectory {}

impl CapType for AssignedPageDirectory {}

impl PhantomCap for AssignedPageDirectory {
    fn phantom_instance() -> Self {
        Self {}
    }
}

impl CopyAliasable for AssignedPageDirectory {
    type CopyOutput = UnassignedPageDirectory;
}

#[derive(Debug)]
pub struct UnassignedPageDirectory {}

impl CapType for UnassignedPageDirectory {}

impl PhantomCap for UnassignedPageDirectory {
    fn phantom_instance() -> Self {
        Self {}
    }
}

impl CopyAliasable for UnassignedPageDirectory {
    type CopyOutput = Self;
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
pub struct MappedPageTable {}

impl CapType for MappedPageTable {}

impl PhantomCap for MappedPageTable {
    fn phantom_instance() -> Self {
        Self {}
    }
}

impl CopyAliasable for MappedPageTable {
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
pub struct MappedPage {}

impl CapType for MappedPage {}

impl PhantomCap for MappedPage {
    fn phantom_instance() -> Self {
        Self {}
    }
}

impl CopyAliasable for MappedPage {
    type CopyOutput = UnmappedPage;
}

impl<FreeSlots: typenum::Unsigned, Role: CNodeRole> CapType for CNode<FreeSlots, Role> {}

mod private {
    pub trait SealedRole {}
    impl SealedRole for super::role::Local {}
    impl SealedRole for super::role::Child {}

    pub trait SealedCapType {}
    impl<BitSize: typenum::Unsigned> SealedCapType for super::Untyped<BitSize> {}
    impl<FreeSlots: typenum::Unsigned, Role: super::CNodeRole> SealedCapType
        for super::CNode<FreeSlots, Role>
    {
    }
    impl SealedCapType for super::ThreadControlBlock {}
    impl SealedCapType for super::Endpoint {}
    impl SealedCapType for super::ASIDControl {}
    impl SealedCapType for super::ASIDPool {}
    impl SealedCapType for super::AssignedPageDirectory {}
    impl SealedCapType for super::UnassignedPageDirectory {}
    impl SealedCapType for super::UnmappedPageTable {}
    impl SealedCapType for super::MappedPageTable {}
    impl SealedCapType for super::UnmappedPage {}
    impl SealedCapType for super::MappedPage {}
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
        Error,
    >
    where
        FreeSlots: Sub<B1>,
        Sub1<FreeSlots>: Unsigned,
        CT: CopyAliasable,
        CT: PhantomCap,
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
            Err(Error::CNodeCopy(err))
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
        Error,
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
            Err(Error::CNodeMove(err))
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
    ) -> Result<(), Error>
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
            Err(Error::CNodeDelete(err))
        } else {
            Ok(())
        }
    }
}
