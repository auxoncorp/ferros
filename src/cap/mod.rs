use core::marker::PhantomData;

use selfe_sys::*;
use typenum::*;

use crate::error::SeL4Error;
use crate::userland::CapRights;

mod asid_pool;
mod badge;
mod cnode;
mod endpoint;
mod irq_control;
mod irq_handler;
mod notification;
mod tcb;
mod untyped;

pub use asid_pool::*;
pub use badge::*;
pub use cnode::*;
pub use endpoint::*;
pub use irq_control::*;
pub use irq_handler::*;
pub use notification::*;
pub use tcb::*;
pub use untyped::*;

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

pub struct CapRange<CT: CapType + PhantomCap, Role: CNodeRole, Slots: Unsigned> {
    pub(crate) start_cptr: usize,
    _cap_type: PhantomData<CT>,
    _role: PhantomData<Role>,
    _slots: PhantomData<Slots>,
}

impl<CT: CapType + PhantomCap, Role: CNodeRole, Slots: Unsigned> CapRange<CT, Role, Slots> {
    pub(crate) fn new(start_cptr: usize) -> Self {
        CapRange {
            start_cptr,
            _cap_type: PhantomData,
            _role: PhantomData,
            _slots: PhantomData,
        }
    }
    pub fn iter(self) -> impl Iterator<Item = Cap<CT, Role>> {
        (0..Slots::USIZE).map(move |offset| Cap {
            cptr: self.start_cptr + offset,
            _role: PhantomData,
            cap_data: PhantomCap::phantom_instance(),
        })
    }

    pub(crate) fn len(&self) -> usize {
        Slots::USIZE
    }
}

impl<CT: CapType + PhantomCap + CopyAliasable, Role: CNodeRole, Slots: Unsigned>
    CapRange<CT, Role, Slots>
{
    pub fn copy(
        &self,
        cnode: &LocalCap<LocalCNode>,
        slots: LocalCNodeSlots<Slots>,
        rights: CapRights,
    ) -> Result<CapRange<CT, Role, Slots>, SeL4Error>
    where
        <CT as CopyAliasable>::CopyOutput: PhantomCap,
    {
        let copied_to_start_cptr = slots.cap_data.offset;
        for (offset, slot) in (0..Slots::USIZE).zip(slots.iter()) {
            let cap: Cap<CT, _> = Cap {
                cptr: self.start_cptr + offset,
                _role: PhantomData,
                cap_data: PhantomCap::phantom_instance(),
            };
            cap.copy(cnode, slot, rights)?;
        }
        Ok(CapRange {
            start_cptr: copied_to_start_cptr,
            _cap_type: PhantomData,
            _role: PhantomData,
            _slots: PhantomData,
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
    impl SealedCapType for Notification {}
    impl<FreeSlots: Unsigned> SealedCapType for ASIDPool<FreeSlots> {}
    impl SealedCapType for IRQControl {}
    impl<IRQ: Unsigned, SetState: IRQSetState> SealedCapType for IRQHandler<IRQ, SetState> where
        IRQ: IsLess<U256, Output = True>
    {
    }

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
        impl<State: PageState> super::SealedCapType for Page<State> {}

        impl<Kind: MemoryKind> super::SealedCapType for UnmappedLargePage<Kind> {}
        impl<Role: CNodeRole, Kind: MemoryKind> super::SealedCapType for MappedLargePage<Role, Kind> {}
        impl<FreePools: Unsigned> super::SealedCapType for ASIDControl<FreePools> {}
        impl super::SealedCapType for UnassignedASID {}
        impl super::SealedCapType for AssignedASID {}

    }

    #[cfg(target_arch = "arm")]
    mod arm {
        use super::super::*;
        use crate::arch::cap::*;
        impl<Kind: MemoryKind> super::SealedCapType for UnmappedSection<Kind> {}
        impl<Role: CNodeRole, Kind: MemoryKind> super::SealedCapType for MappedSection<Role, Kind> {}

        impl<Kind: MemoryKind> super::SealedCapType for UnmappedSuperSection<Kind> {}
        impl<Role: CNodeRole, Kind: MemoryKind> super::SealedCapType for MappedSuperSection<Role, Kind> {}
    }
}
