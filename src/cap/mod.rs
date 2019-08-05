use core::marker::PhantomData;

use typenum::*;

use crate::error::SeL4Error;
mod asid;
mod asid_control;
mod asid_pool;
mod badge;
mod cnode;
mod endpoint;
mod fault_reply_endpoint;
mod irq_control;
pub mod irq_handler;
mod notification;
mod page;
mod page_table;
mod tcb;
mod untyped;

pub use asid::*;
pub use asid_control::*;
pub use asid_pool::*;
pub use badge::*;
pub use cnode::*;
pub use endpoint::*;
pub use fault_reply_endpoint::*;
pub use irq_control::*;
pub use irq_handler::*;
pub use notification::*;
pub use page::*;
pub use page_table::*;
use selfe_wrap::{CNodeCptr, CNodeKernel, CapIndex, CapRights, FullyQualifiedCptr, SelfeKernel};
pub use tcb::*;
pub use untyped::*;

/// Type-level enum indicating the relative location / Capability Pointer addressing
/// scheme that should be used for the objects parameterized by it.
pub trait CNodeRole: private::SealedRole {
    fn to_index(raw_cptr_offset: usize) -> CapIndex;
}

pub mod role {
    use super::{CNodeRole, CapIndex};

    #[derive(Debug, PartialEq)]
    pub struct Local {}
    impl CNodeRole for Local {
        fn to_index(raw_cptr_offset: usize) -> CapIndex {
            CapIndex::Local(raw_cptr_offset.into())
        }
    }

    #[derive(Debug, PartialEq)]
    pub struct Child {}
    impl CNodeRole for Child {
        fn to_index(raw_cptr_offset: usize) -> CapIndex {
            CapIndex::Child(raw_cptr_offset.into())
        }
    }
}

/// Marker trait for CapType implementing structs to indicate that
/// this type of capability can be generated directly
/// from retyping an Untyped
pub trait DirectRetype {
    type SizeBits: Unsigned;
    fn sel4_type_id() -> usize;
}

/// Marker trait for CapType implementing structs to indicate that
/// instances of this type of capability can be copied and aliased safely
/// when done through the use of this API
pub trait CopyAliasable {
    type CopyOutput: CapType + for<'a> From<&'a Self>;
}

/// Marker trait for CapType implementing structs to indicate that
/// instances of this type of capability can be copied and aliased safely
/// when done through the use of this API, and furthermore can be
/// granted badges
pub trait Mintable: CopyAliasable {}

/// Internal marker trait for CapType implementing structs that can
/// have meaningful instances created for them purely from
/// their type signatures.
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

pub struct CapRange<CT: CapType, Role: CNodeRole, Slots: Unsigned> {
    pub(crate) start_cptr: usize,
    pub(crate) start_cap_data: CT,
    _role: PhantomData<Role>,
    _slots: PhantomData<Slots>,
}

impl<CT: CapType + PhantomCap, Role: CNodeRole, Slots: Unsigned> CapRange<CT, Role, Slots> {
    pub(crate) fn new_phantom(start_cptr: usize) -> Self {
        CapRange {
            start_cptr,
            start_cap_data: CT::phantom_instance(),
            _role: PhantomData,
            _slots: PhantomData,
        }
    }
}
impl<CT: CapType, Role: CNodeRole, Slots: Unsigned> CapRange<CT, Role, Slots> {
    pub(crate) fn new(start_cptr: usize, start_cap_data: CT) -> Self {
        CapRange {
            start_cptr,
            start_cap_data,
            _role: PhantomData,
            _slots: PhantomData,
        }
    }

    pub(crate) fn into_iter(self) -> impl Iterator<Item = Cap<CT, Role>>
    where
        CT: CapRangeDataReconstruction,
    {
        (0..self.len()).map(move |index| Cap {
            cptr: self.start_cptr + index,
            _role: PhantomData,
            cap_data: CT::reconstruct(index, &self.start_cap_data),
        })
    }

    pub(crate) fn len(&self) -> usize {
        Slots::USIZE
    }

    pub fn weaken(self) -> WeakCapRange<CT, Role> {
        let len = self.len();
        WeakCapRange::new(self.start_cptr, self.start_cap_data, len)
    }
}

impl<CT: CapType + CopyAliasable, Role: CNodeRole, Slots: Unsigned> CapRange<CT, Role, Slots> {
    pub fn copy<DestRole: CNodeRole>(
        &self,
        cnode: &LocalCap<CNode<Role>>,
        slots: CNodeSlots<Slots, DestRole>,
        rights: CapRights,
    ) -> Result<CapRange<CT::CopyOutput, DestRole, Slots>, SeL4Error>
    where
        CT: CapRangeDataReconstruction,
    {
        let copied_to_start_cptr = slots.cap_data.offset;
        // N.B. Conside replacing with a general purpose CapRange::iter(&self) that returns references to constructed caps
        for (offset, slot) in (0..Slots::USIZE).zip(slots.iter()) {
            let cap: Cap<CT, Role> = Cap {
                cptr: self.start_cptr + offset,
                _role: PhantomData,
                cap_data: CapRangeDataReconstruction::reconstruct(offset, &self.start_cap_data),
            };
            cap.copy(cnode, slot, rights)?;
        }
        Ok(CapRange {
            start_cptr: copied_to_start_cptr,
            start_cap_data: From::from(&self.start_cap_data),
            _role: PhantomData,
            _slots: PhantomData,
        })
    }
}

pub struct WeakCapRange<CT: CapType, Role: CNodeRole> {
    pub(crate) start_cptr: usize,
    pub(crate) start_cap_data: CT,
    pub(crate) len: usize,
    _role: PhantomData<Role>,
}

#[derive(Debug, PartialEq)]
pub enum WeakCopyError {
    NotEnoughSlots,
    SeL4Error(SeL4Error),
}

impl From<SeL4Error> for WeakCopyError {
    fn from(e: SeL4Error) -> Self {
        WeakCopyError::SeL4Error(e)
    }
}

impl<CT: CapType + CopyAliasable, Role: CNodeRole> WeakCapRange<CT, Role> {
    pub fn copy<DestRole: CNodeRole>(
        &self,
        cnode: &LocalCap<CNode<Role>>,
        slots: &mut LocalCap<WCNodeSlotsData<DestRole>>,
        rights: CapRights,
    ) -> Result<WeakCapRange<<CT as CopyAliasable>::CopyOutput, DestRole>, WeakCopyError>
    where
        CT: CapRangeDataReconstruction,
    {
        if slots.size() < self.len() {
            return Err(WeakCopyError::NotEnoughSlots);
        }
        let copied_to_start_cptr = slots.cap_data.offset;
        // N.B. Conside replacing with a general purpose CapRange::iter(&self) that returns references to constructed caps
        for (offset, slot) in (0..self.len()).zip(slots.incrementally_consuming_iter()) {
            let cap: Cap<CT, Role> = Cap {
                cptr: self.start_cptr + offset,
                _role: PhantomData,
                cap_data: CapRangeDataReconstruction::reconstruct(offset, &self.start_cap_data),
            };
            cap.copy(cnode, slot, rights)?;
        }
        Ok(WeakCapRange::new(
            copied_to_start_cptr,
            From::from(&self.start_cap_data),
            self.len(),
        ))
    }
}
impl<CT: CapType, Role: CNodeRole> WeakCapRange<CT, Role> {
    pub(crate) fn new(start_cptr: usize, start_cap_data: CT, len: usize) -> Self {
        WeakCapRange {
            start_cptr,
            start_cap_data,
            len,
            _role: PhantomData,
        }
    }

    pub(crate) fn into_iter(self) -> impl Iterator<Item = Cap<CT, Role>>
    where
        CT: CapRangeDataReconstruction,
    {
        (0..self.len()).map(move |index| Cap {
            cptr: self.start_cptr + index,
            _role: PhantomData,
            cap_data: CT::reconstruct(index, &self.start_cap_data),
        })
    }

    pub(crate) fn len(&self) -> usize {
        self.len
    }
}

/// A helper trait for CapRange and WeakCapRange to assist in iteration.
///
/// Represents a CapType for which instances in a collection
/// can be reconstructed using only a single seed/starting instance reference
/// and the index-of-iteration.
pub trait CapRangeDataReconstruction {
    fn reconstruct(index: usize, seed: &Self) -> Self;
}

impl<Role: CNodeRole, CT: CapType> Cap<CT, Role> {
    /// Copy a capability from one CNode to another CNode
    pub fn copy<DestRole: CNodeRole>(
        &self,
        src_cnode: &LocalCap<CNode<Role>>,
        dest_slot: CNodeSlot<DestRole>,
        rights: CapRights,
    ) -> Result<Cap<CT::CopyOutput, DestRole>, SeL4Error>
    where
        CT: CopyAliasable,
    {
        let dest_offset = self.unchecked_copy(src_cnode, dest_slot, rights)?;
        Ok(Cap {
            cptr: dest_offset,
            cap_data: From::from(&self.cap_data),
            _role: PhantomData,
        })
    }

    /// Super dangerous! Not for public use.
    ///
    /// Returns the destination offset
    pub(crate) fn unchecked_copy<DestRole: CNodeRole>(
        &self,
        src_cnode: &LocalCap<CNode<Role>>,
        dest_slot: CNodeSlot<DestRole>,
        rights: CapRights,
    ) -> Result<usize, SeL4Error> {
        let source = FullyQualifiedCptr {
            cnode: src_cnode.into(),
            index: Role::to_index(self.cptr),
        };
        SelfeKernel::cnode_copy(&source, dest_slot.elim().cptr, rights)
            .map(|dest| dest.index.into())
    }

    /// Copy a capability to another CNode while also setting rights and a badge
    pub fn mint<DestRole: CNodeRole>(
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
        let source = FullyQualifiedCptr {
            cnode: src_cnode.into(),
            index: Role::to_index(self.cptr),
        };
        SelfeKernel::cnode_mint(&source, dest_slot.elim().cptr, rights, badge.into()).map(
            |destination| Cap {
                cptr: destination.index.into(),
                cap_data: PhantomCap::phantom_instance(),
                _role: PhantomData,
            },
        )
    }

    /// Copy a capability to another slot inside the same CNode while also setting rights and a badge
    pub fn mint_inside_cnode(
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
        let source = FullyQualifiedCptr {
            // Because we limit the dest_slot to be local, and CNodeSlots
            // are tracked using their primary cptr as a reference to the
            // relevant CNode, we can extract the cnode cptr thusly
            cnode: CNodeCptr(dest_slot.cptr.into()),
            index: Role::to_index(self.cptr),
        };
        SelfeKernel::cnode_mint(&source, dest_slot.elim().cptr, rights, badge.into()).map(
            |destination| Cap {
                cptr: destination.index.into(),
                cap_data: PhantomCap::phantom_instance(),
                _role: PhantomData,
            },
        )
    }

    /// Migrate a capability from one CNode slot to another.
    pub fn move_to_slot<DestRole: CNodeRole>(
        self,
        src_cnode: &LocalCap<CNode<Role>>,
        dest_slot: CNodeSlot<DestRole>,
    ) -> Result<Cap<CT, DestRole>, SeL4Error>
    where
        CT: Movable,
    {
        let source = FullyQualifiedCptr {
            cnode: src_cnode.into(),
            index: Role::to_index(self.cptr),
        };
        SelfeKernel::cnode_move(&source, dest_slot.elim().cptr).map(|destination| Cap {
            cptr: destination.index.into(),
            cap_data: self.cap_data,
            _role: PhantomData,
        })
    }

    /// Delete a capability
    pub fn delete(self, parent_cnode: &LocalCap<CNode<Role>>) -> Result<(), SeL4Error>
    where
        CT: Delible,
    {
        SelfeKernel::cnode_delete(FullyQualifiedCptr {
            cnode: parent_cnode.into(),
            index: Role::to_index(self.cptr),
        })
        .map(|_| ())
    }
}

mod private {
    use super::*;

    pub trait SealedRole {}
    impl private::SealedRole for role::Local {}
    impl private::SealedRole for role::Child {}

    pub trait SealedCapType {}
    impl<BitSize: typenum::Unsigned, Kind: MemoryKind> SealedCapType for Untyped<BitSize, Kind> {}
    impl<Role: CNodeRole> SealedCapType for CNode<Role> {}
    impl<Size: Unsigned, Role: CNodeRole> SealedCapType for CNodeSlotsData<Size, Role> {}
    impl SealedCapType for ThreadControlBlock {}
    impl SealedCapType for ThreadPriorityAuthority {}
    impl SealedCapType for Endpoint {}
    impl SealedCapType for FaultReplyEndpoint {}
    impl SealedCapType for Notification {}
    impl<FreeSlots: Unsigned> SealedCapType for ASIDPool<FreeSlots> {}
    impl SealedCapType for IRQControl {}
    impl<IRQ: Unsigned, SetState: IRQSetState> SealedCapType for IRQHandler<IRQ, SetState> where
        IRQ: IsLess<MaxIRQCount, Output = True>
    {
    }
    impl<State: PageState> SealedCapType for Page<State> {}

    /*
    Cross Arch things:

    | seL4 calls it | Rust calls it |
    |---------------|---------------|
    |    aarch32    |      arm      |
    |    aarch64    |    aarch64    |
    |     IA-32     |      x86      |
    |      x64      |    x86_64     |
    |               |    powerpc    |
    |               |   powerpc64   |
    | RISC-V 32-bit |               |
    | RISC-V 64-bit |               |

    Cf.
      - seL4 Manual 7.1.1
      - https://doc.rust-lang.org/reference/conditional-compilation.html#target_arch

    Notes:

    1. All of the supported architectures have pages, large pages,
       page tables, and all but RISC-V have page
       directories. However, they are reënumerated for each
       architecture on account of their unique `DirectRetype`
       implementations which use an associated type for `SizeBits`
       and use the architecture specific seL4 type id. (Cf. seL4
       Manual 7.1.3)

    2. RISC-V's address space structure consists of only page
       tables. If there someday exists first-class support for
       RISC-V in Rust, we will need to pluck page directories out
       from the `arch` mod and move them into the architecture
       mods which use them. (Cf. seL4 Manual 7.1.1)
    */
    mod arch {
        use super::super::*;
        use crate::arch::cap::*;
        impl super::SealedCapType for PageDirectory {}
        impl super::SealedCapType for PageTable {}

        impl<FreePools: Unsigned> super::SealedCapType for ASIDControl<FreePools> {}
        impl super::SealedCapType for UnassignedASID {}
        impl super::SealedCapType for AssignedASID {}

    }
}
