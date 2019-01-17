use core::marker::PhantomData;
use core::ops::Sub;
use crate::userland::{role, CNode, CNodeRole, Error};
use sel4_sys::*;
use typenum::operator_aliases::Sub1;
use typenum::{Unsigned, B1};

// TODO: this is more specifically "fixed size and also not a funny vspace thing"
pub trait FixedSizeCap {}

#[derive(Debug)]
pub struct Cap<CT: CapType, Role: CNodeRole> {
    pub cptr: usize,
    pub(super) _cap_type: PhantomData<CT>,
    pub(super) _role: PhantomData<Role>,
}

pub trait CapType: private::SealedCapType {
    type CopyOutput: CapType;
    fn sel4_type_id() -> usize;
}

impl<CT: CapType, Role: CNodeRole> Cap<CT, Role> {
    // TODO most of this should only happen in the bootstrap adapter
    pub fn wrap_cptr(cptr: usize) -> Cap<CT, Role> {
        Cap {
            cptr: cptr,
            _cap_type: PhantomData,
            _role: PhantomData,
        }
    }
}

#[derive(Debug)]
pub struct Untyped<BitSize: Unsigned> {
    _bit_size: PhantomData<BitSize>,
}

impl<BitSize: Unsigned> CapType for Untyped<BitSize> {
    type CopyOutput = Self;
    fn sel4_type_id() -> usize {
        api_object_seL4_UntypedObject as usize
    }
}

#[derive(Debug)]
pub struct ThreadControlBlock {}

impl CapType for ThreadControlBlock {
    type CopyOutput = Self;
    fn sel4_type_id() -> usize {
        api_object_seL4_TCBObject as usize
    }
}

impl FixedSizeCap for ThreadControlBlock {}

// asid control
#[derive(Debug)]
pub struct ASIDControl {}

impl CapType for ASIDControl {
    type CopyOutput = Self;
    fn sel4_type_id() -> usize {
        0 // TODO WUT
    }
}

// asid pool
// TODO: track capacity with the types
// TODO: track in the pagedirectory type whether it has been assigned (mapped), and for pagetable too
#[derive(Debug)]
pub struct ASIDPool {}

impl CapType for ASIDPool {
    type CopyOutput = Self;
    fn sel4_type_id() -> usize {
        0 // TODO put type_id in a 'retypable' trait?
    }
}

#[derive(Debug)]
pub struct Endpoint {}

impl CapType for Endpoint {
    type CopyOutput = Self;
    fn sel4_type_id() -> usize {
        api_object_seL4_EndpointObject as usize
    }
}

impl FixedSizeCap for Endpoint {}

#[derive(Debug)]
pub struct AssignedPageDirectory {}

impl CapType for AssignedPageDirectory {
    type CopyOutput = UnassignedPageDirectory;
    fn sel4_type_id() -> usize {
        _object_seL4_ARM_PageDirectoryObject as usize
    }
}

#[derive(Debug)]
pub struct UnassignedPageDirectory {}

impl CapType for UnassignedPageDirectory {
    type CopyOutput = Self;
    fn sel4_type_id() -> usize {
        _object_seL4_ARM_PageDirectoryObject as usize
    }
}

impl FixedSizeCap for UnassignedPageDirectory {}

#[derive(Debug)]
pub struct UnmappedPageTable {}

impl CapType for UnmappedPageTable {
    type CopyOutput = Self;
    fn sel4_type_id() -> usize {
        _object_seL4_ARM_PageTableObject as usize
    }
}

impl FixedSizeCap for UnmappedPageTable {}

#[derive(Debug)]
pub struct MappedPageTable {}

impl CapType for MappedPageTable {
    type CopyOutput = UnmappedPageTable;
    fn sel4_type_id() -> usize {
        _object_seL4_ARM_PageTableObject as usize
    }
}

#[derive(Debug)]
pub struct UnmappedPage {}

impl CapType for UnmappedPage {
    type CopyOutput = Self;
    fn sel4_type_id() -> usize {
        _object_seL4_ARM_SmallPageObject as usize
    }
}

impl FixedSizeCap for UnmappedPage {}

#[derive(Debug)]
pub struct MappedPage {}

impl CapType for MappedPage {
    type CopyOutput = UnmappedPage;
    fn sel4_type_id() -> usize {
        _object_seL4_ARM_SmallPageObject as usize
    }
}

mod private {
    pub trait SealedCapType {}
    impl<BitSize: typenum::Unsigned> SealedCapType for super::Untyped<BitSize> {}
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
    pub fn copy_local<SourceFreeSlots: Unsigned, FreeSlots: Unsigned>(
        &self,
        src_cnode: &CNode<SourceFreeSlots, role::Local>,
        dest_cnode: CNode<FreeSlots, role::Local>,
        rights: seL4_CapRights_t,
    ) -> Result<
        (
            Cap<CT::CopyOutput, role::Local>,
            CNode<Sub1<FreeSlots>, role::Local>,
        ),
        Error,
    >
    where
        FreeSlots: Sub<B1>,
        Sub1<FreeSlots>: Unsigned,
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
                rights,              // rights
            )
        };

        if err != 0 {
            Err(Error::CNodeCopy(err))
        } else {
            Ok((
                Cap {
                    cptr: dest_slot.offset,
                    _cap_type: PhantomData,
                    _role: PhantomData,
                },
                dest_cnode,
            ))
        }
    }

    pub fn copy_child<SourceFreeSlots: Unsigned, FreeSlots: Unsigned>(
        &self,
        src_cnode: &CNode<SourceFreeSlots, role::Local>,
        dest_cnode: CNode<FreeSlots, role::Child>,
        rights: seL4_CapRights_t,
    ) -> Result<
        (
            Cap<CT::CopyOutput, role::Child>,
            CNode<Sub1<FreeSlots>, role::Child>,
        ),
        Error,
    >
    where
        FreeSlots: Sub<B1>,
        Sub1<FreeSlots>: Unsigned,
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
                rights,              // rights
            )
        };

        if err != 0 {
            Err(Error::CNodeCopy(err))
        } else {
            Ok((
                Cap {
                    cptr: dest_slot.offset,
                    _cap_type: PhantomData,
                    _role: PhantomData,
                },
                dest_cnode,
            ))
        }
    }
}
