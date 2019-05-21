use core::marker::PhantomData;

use selfe_sys::*;
use typenum::*;

use crate::error::SeL4Error;
use crate::userland::{Badge, CNodeSlot, CapRights, LocalCNode, LocalCNodeSlot};

/// Type-level enum indicating the relative location / Capability Pointer addressing
/// scheme that should be used for the objects parameterized by it.
pub trait CNodeRole {}

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
    pub(crate) _role: PhantomData<Role>,
}

pub trait CapType {}

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

pub struct CapRange<CT: CapType + PhantomCap, Role: CNodeRole, Slots: Unsigned> {
    pub(crate) start_cptr: usize,
    pub(crate) _cap_type: PhantomData<CT>,
    pub(crate) _role: PhantomData<Role>,
    pub(crate) _slots: PhantomData<Slots>,
}

impl<CT: CapType + PhantomCap, Role: CNodeRole, Slots: Unsigned> CapRange<CT, Role, Slots> {
    pub fn iter(self) -> impl Iterator<Item = Cap<CT, Role>> {
        (0..Slots::USIZE).map(move |offset| Cap {
            cptr: self.start_cptr + offset,
            _role: PhantomData,
            cap_data: PhantomCap::phantom_instance(),
        })
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
        let dest_offset = self.unchecked_copy(src_cnode, dest_slot, rights)?;
        Ok(Cap {
            cptr: dest_offset,
            cap_data: PhantomCap::phantom_instance(),
            _role: PhantomData,
        })
    }

    /// Super dangerous! Not for public use.
    ///
    /// Returns the destination offset
    pub(crate) fn unchecked_copy<DestRole: CNodeRole>(
        &self,
        src_cnode: &LocalCap<LocalCNode>,
        dest_slot: CNodeSlot<DestRole>,
        rights: CapRights,
    ) -> Result<usize, SeL4Error> {
        let (dest_cptr, dest_offset, _) = dest_slot.elim();
        let err = unsafe {
            seL4_CNode_Copy(
                dest_cptr,           // _service
                dest_offset,         // index
                seL4_WordBits as u8, // depth
                // Since src_cnode is restricted to CSpace Local Root, the cptr must
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
            Ok(dest_offset)
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
