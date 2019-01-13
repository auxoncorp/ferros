use core::marker::PhantomData;
use core::mem::{size_of, transmute};
use core::ops::{Add, Sub};
use sel4_sys::*;
use typenum::operator_aliases::{Add1, Diff, Shleft, Sub1};
use typenum::{
    Bit, Exp, IsGreaterOrEqual, UInt, UTerm, Unsigned, B0, B1, U1, U1024, U12, U19, U2, U24, U256,
    U3, U32, U5, U8,
};

use crate::pow::{Pow, _Pow};

#[derive(Debug)]
pub enum Error {
    UntypedRetype(u32),
    TCBConfigure(u32),
    MapPageTable(u32),
    UnmapPageTable(u32),
    ASIDPoolAssign(u32),
    MapPage(u32),
    UnmapPage(u32),
    CNodeCopy(u32),
}

pub trait CapType {
    type CopyOutput: CapType;
    fn sel4_type_id() -> usize;
}

// TODO: this is more specifically "fixed size and also not a funny vspace thing"
pub trait FixedSizeCap {}

#[derive(Debug)]
pub struct Capability<CT: CapType> {
    pub cptr: usize,
    _cap_type: PhantomData<CT>,
}

#[derive(Debug)]
pub struct ChildCapability<CT: CapType> {
    child_cptr: usize,
    _cap_type: PhantomData<CT>,
}

pub trait CNodeRole {}
pub mod CNodeRoles {
    use super::CNodeRole;

    #[derive(Debug)]
    pub struct CSpaceRoot {}
    impl CNodeRole for CSpaceRoot {}

    #[derive(Debug)]
    pub struct ChildProcess {}
    impl CNodeRole for ChildProcess {}
}

/// There will only ever be one CNode in a process with Role = CSpaceRoot. The
/// cptrs any regular Capability are /also/ offsets into that cnode, because of
/// how we're configuring each CNode's guard.
#[derive(Debug)]
pub struct CNode<FreeSlots: Unsigned, Role: CNodeRole> {
    radix: u8,
    next_free_slot: usize,
    cptr: usize,
    _free_slots: PhantomData<FreeSlots>,
    _role: PhantomData<Role>,
}

#[derive(Debug)]
struct CNodeSlot {
    cptr: usize,
    offset: usize,
}

impl<FreeSlots: Unsigned, Role: CNodeRole> CNode<FreeSlots, Role> {
    // TODO: reverse these args to be consistent with everything else
    fn consume_slot(self) -> (CNode<Sub1<FreeSlots>, Role>, CNodeSlot)
    where
        FreeSlots: Sub<B1>,
        Sub1<FreeSlots>: Unsigned,
    {
        (
            // TODO: use mem::transmute
            CNode {
                radix: self.radix,
                next_free_slot: self.next_free_slot + 1,
                cptr: self.cptr,
                _free_slots: PhantomData,
                _role: PhantomData,
            },
            CNodeSlot {
                cptr: self.cptr,
                offset: self.next_free_slot,
            },
        )
    }

    // Reserve Count slots. Return another node with the same cptr, but the
    // requested capacity.
    pub fn reserve_region<Count: Unsigned>(
        self,
    ) -> (CNode<Count, Role>, CNode<Diff<FreeSlots, Count>, Role>)
    where
        FreeSlots: Sub<Count>,
        Diff<FreeSlots, Count>: Unsigned,
    {
        (
            CNode {
                radix: self.radix,
                next_free_slot: self.next_free_slot,
                cptr: self.cptr,
                _free_slots: PhantomData,
                _role: PhantomData,
            },
            // TODO: use mem::transmute
            CNode {
                radix: self.radix,
                next_free_slot: self.next_free_slot + Count::to_usize(),
                cptr: self.cptr,
                _free_slots: PhantomData,
                _role: PhantomData,
            },
        )
    }

    pub fn reservation_iter<Count: Unsigned>(
        self,
    ) -> (
        impl Iterator<Item = CNode<U1, Role>>,
        CNode<Diff<FreeSlots, Count>, Role>,
    )
    where
        FreeSlots: Sub<Count>,
        Diff<FreeSlots, Count>: Unsigned,
    {
        let iter_radix = self.radix;
        let iter_cptr = self.cptr;
        (
            (self.next_free_slot..self.next_free_slot + Count::to_usize()).map(move |slot| CNode {
                radix: iter_radix,
                next_free_slot: slot,
                cptr: iter_cptr,
                _free_slots: PhantomData,
                _role: PhantomData,
            }),
            // TODO: use mem::transmute
            CNode {
                radix: self.radix,
                next_free_slot: self.next_free_slot + Count::to_usize(),
                cptr: self.cptr,
                _free_slots: PhantomData,
                _role: PhantomData,
            },
        )
    }
}

// TODO: how many slots are there really? We should be able to know this at build
// time.
// Answer: The radix is 19, and there are 12 initial caps. But there are also a bunch
// of random things in the bootinfo.
// TODO: ideally, this should only be callable once in the process. Is that possible?
pub fn root_cnode(bootinfo: &'static seL4_BootInfo) -> CNode<U1024, CNodeRoles::CSpaceRoot> {
    CNode {
        radix: 19,
        next_free_slot: 1000, // TODO: look at the bootinfo to determine the real value
        cptr: seL4_CapInitThreadCNode as usize,
        _free_slots: PhantomData,
        _role: PhantomData,
    }
}

impl<CT: CapType> Capability<CT> {
    pub fn copy_local<SourceFreeSlots: Unsigned, FreeSlots: Unsigned>(
        &self,
        src_cnode: &CNode<SourceFreeSlots, CNodeRoles::CSpaceRoot>,
        dest_cnode: CNode<FreeSlots, CNodeRoles::CSpaceRoot>,
        rights: seL4_CapRights_t,
    ) -> Result<
        (
            Capability<CT::CopyOutput>,
            CNode<Sub1<FreeSlots>, CNodeRoles::CSpaceRoot>,
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
                dest_slot.cptr as u32,   // _service
                dest_slot.offset as u32, // index
                CONFIG_WORD_SIZE as u8,  // depth
                // Since src_cnode is restricted to CSpaceRoot, the cptr must
                // actually be the slot index
                src_cnode.cptr as u32,                      // src_root
                self.cptr as u32,       // src_index
                CONFIG_WORD_SIZE as u8, // src_depth
                rights,                 // rights
            )
        };

        if err != 0 {
            Err(Error::CNodeCopy(err))
        } else {
            Ok((
                Capability {
                    cptr: dest_slot.offset,
                    _cap_type: PhantomData,
                },
                dest_cnode,
            ))
        }
    }
}

/////////////
// Untyped //
/////////////

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

pub fn wrap_untyped<BitSize: Unsigned>(
    cptr: usize,
    untyped_desc: &seL4_UntypedDesc,
) -> Option<Capability<Untyped<BitSize>>> {
    if untyped_desc.sizeBits == BitSize::to_u8() {
        Some(Capability {
            cptr,
            _cap_type: PhantomData,
        })
    } else {
        None
    }
}

impl<BitSize: Unsigned> Capability<Untyped<BitSize>> {
    pub fn split<FreeSlots: Unsigned>(
        self,
        dest_cnode: CNode<FreeSlots, CNodeRoles::CSpaceRoot>,
    ) -> Result<
        (
            Capability<Untyped<Sub1<BitSize>>>,
            Capability<Untyped<Sub1<BitSize>>>,
            CNode<Sub1<FreeSlots>, CNodeRoles::CSpaceRoot>,
        ),
        Error,
    >
    where
        FreeSlots: Sub<B1>,
        Sub1<FreeSlots>: Unsigned,

        BitSize: Sub<B1>,
        Sub1<BitSize>: Unsigned,
    {
        let (dest_cnode, dest_slot) = dest_cnode.consume_slot();

        let err = unsafe {
            seL4_Untyped_Retype(
                self.cptr as u32,                          // _service
                Untyped::<BitSize>::sel4_type_id() as u32, // type
                (BitSize::to_u32() - 1),                   // size_bits
                dest_slot.cptr as u32,                     // root
                0,                                         // index
                0,                                         // depth
                dest_slot.offset as u32,                   // offset
                1,                                         // num_objects
            )
        };
        if err != 0 {
            return Err(Error::UntypedRetype(err));
        }

        Ok((
            Capability {
                cptr: self.cptr,
                _cap_type: PhantomData,
            },
            Capability {
                cptr: dest_slot.offset,
                _cap_type: PhantomData,
            },
            dest_cnode,
        ))
    }

    pub fn quarter<FreeSlots: Unsigned>(
        self,
        dest_cnode: CNode<FreeSlots, CNodeRoles::CSpaceRoot>,
    ) -> Result<
        (
            Capability<Untyped<Diff<BitSize, U2>>>,
            Capability<Untyped<Diff<BitSize, U2>>>,
            Capability<Untyped<Diff<BitSize, U2>>>,
            Capability<Untyped<Diff<BitSize, U2>>>,
            CNode<Sub1<Sub1<Sub1<FreeSlots>>>, CNodeRoles::CSpaceRoot>,
        ),
        Error,
    >
    where
        FreeSlots: Sub<U3>,
        Diff<FreeSlots, U3>: Unsigned,

        FreeSlots: Sub<B1>,
        Sub1<FreeSlots>: Unsigned,

        Sub1<FreeSlots>: Sub<B1>,
        Sub1<Sub1<FreeSlots>>: Unsigned,

        Sub1<Sub1<FreeSlots>>: Sub<B1>,
        Sub1<Sub1<Sub1<FreeSlots>>>: Unsigned,

        BitSize: Sub<U2>,
        Diff<BitSize, U2>: Unsigned,
    {
        // TODO: use reserve_range here
        let (dest_cnode, dest_slot1) = dest_cnode.consume_slot();
        let (dest_cnode, dest_slot2) = dest_cnode.consume_slot();
        let (dest_cnode, dest_slot3) = dest_cnode.consume_slot();

        let err = unsafe {
            seL4_Untyped_Retype(
                self.cptr as u32,                          // _service
                Untyped::<BitSize>::sel4_type_id() as u32, // type
                (BitSize::to_u32() - 2),                   // size_bits
                dest_slot1.cptr as u32,                    // root
                0,                                         // index
                0,                                         // depth
                dest_slot1.offset as u32,                  // offset
                3,                                         // num_objects
            )
        };
        if err != 0 {
            return Err(Error::UntypedRetype(err));
        }

        Ok((
            Capability {
                cptr: self.cptr,
                _cap_type: PhantomData,
            },
            Capability {
                cptr: dest_slot1.offset,
                _cap_type: PhantomData,
            },
            Capability {
                cptr: dest_slot2.offset,
                _cap_type: PhantomData,
            },
            Capability {
                cptr: dest_slot3.offset,
                _cap_type: PhantomData,
            },
            dest_cnode,
        ))
    }

    // TODO add required bits as an associated type for each TargetCapType, require that
    // this untyped is big enough
    pub fn retype_local<FreeSlots: Unsigned, TargetCapType: CapType>(
        self,
        dest_cnode: CNode<FreeSlots, CNodeRoles::CSpaceRoot>,
    ) -> Result<
        (
            Capability<TargetCapType>,
            CNode<Sub1<FreeSlots>, CNodeRoles::CSpaceRoot>,
        ),
        Error,
    >
    where
        FreeSlots: Sub<B1>,
        Sub1<FreeSlots>: Unsigned,
        TargetCapType: FixedSizeCap,
    {
        let (dest_cnode, dest_slot) = dest_cnode.consume_slot();

        let err = unsafe {
            seL4_Untyped_Retype(
                self.cptr as u32,                     // _service
                TargetCapType::sel4_type_id() as u32, // type
                0,                                    // size_bits
                dest_slot.cptr as u32,                // root
                0,                                    // index
                0,                                    // depth
                dest_slot.offset as u32,              // offset
                1,                                    // num_objects
            )
        };

        if err != 0 {
            return Err(Error::UntypedRetype(err));
        }

        Ok((
            Capability {
                cptr: dest_slot.offset,
                _cap_type: PhantomData,
            },
            dest_cnode,
        ))
    }

    // TODO: the required size of the untyped depends in some way on the child radix, but HOW?
    // answer: it needs 4 more bits, this value is seL4_SlotBits.
    pub fn retype_local_cnode<FreeSlots: Unsigned, ChildRadix: Unsigned>(
        self,
        dest_cnode: CNode<FreeSlots, CNodeRoles::CSpaceRoot>,
    ) -> Result<
        (
            CNode<Pow<ChildRadix>, CNodeRoles::ChildProcess>,
            CNode<Sub1<FreeSlots>, CNodeRoles::CSpaceRoot>,
        ),
        Error,
    >
    where
        FreeSlots: Sub<B1>,
        Sub1<FreeSlots>: Unsigned,
        ChildRadix: _Pow,
        Pow<ChildRadix>: Unsigned,
    {
        let (dest_cnode, dest_slot) = dest_cnode.consume_slot();

        let err = unsafe {
            seL4_Untyped_Retype(
                self.cptr as u32,               // _service
                api_object_seL4_CapTableObject, // type
                ChildRadix::to_u32(),           // size_bits
                dest_slot.cptr as u32,          // root
                0,                              // index
                0,                              // depth
                dest_slot.offset as u32,        // offset
                1,                              // num_objects
            )
        };

        if err != 0 {
            return Err(Error::UntypedRetype(err));
        }

        Ok((
            CNode {
                radix: ChildRadix::to_u8(),
                next_free_slot: 0,
                cptr: dest_slot.offset,
                _free_slots: PhantomData,
                _role: PhantomData,
            },
            dest_cnode,
        ))
    }

    pub fn retype_child<FreeSlots: Unsigned, TargetCapType: CapType>(
        self,
        dest_cnode: CNode<FreeSlots, CNodeRoles::ChildProcess>,
    ) -> Result<
        (
            ChildCapability<TargetCapType>,
            CNode<Sub1<FreeSlots>, CNodeRoles::ChildProcess>,
        ),
        Error,
    >
    where
        FreeSlots: Sub<B1>,
        Sub1<FreeSlots>: Unsigned,
        TargetCapType: FixedSizeCap,
    {
        let (dest_cnode, dest_slot) = dest_cnode.consume_slot();

        let err = unsafe {
            seL4_Untyped_Retype(
                self.cptr as u32,                     // _service
                TargetCapType::sel4_type_id() as u32, // type
                0,                                    // size_bits
                dest_slot.cptr as u32,                // root
                0,                                    // index
                0,                                    // depth
                dest_slot.offset as u32,              // offset
                1,                                    // num_objects
            )
        };

        if err != 0 {
            return Err(Error::UntypedRetype(err));
        }

        Ok((
            ChildCapability {
                child_cptr: dest_slot.offset,
                _cap_type: PhantomData,
            },
            dest_cnode,
        ))
    }
}

// The ASID pool needs an untyped of exactly 4k
impl Capability<Untyped<U12>> {
    // TODO put retype local into a trait so we can dispatch via the target cap type
    pub fn retype_asid_pool<FreeSlots: Unsigned>(
        self,
        asid_control: Capability<ASIDControl>,
        dest_cnode: CNode<FreeSlots, CNodeRoles::CSpaceRoot>,
    ) -> Result<
        (
            Capability<ASIDPool>,
            CNode<Sub1<FreeSlots>, CNodeRoles::CSpaceRoot>,
        ),
        Error,
    >
    where
        FreeSlots: Sub<B1>,
        Sub1<FreeSlots>: Unsigned,
    {
        let (dest_cnode, dest_slot) = dest_cnode.consume_slot();

        let err = unsafe {
            seL4_ARM_ASIDControl_MakePool(
                asid_control.cptr as u32,       // _service
                self.cptr as u32,               // untyped
                dest_slot.cptr as u32,          // root
                dest_slot.offset as u32,        // index
                (8 * size_of::<usize>()) as u8, // depth
            )
        };

        if err != 0 {
            return Err(Error::UntypedRetype(err));
        }

        Ok((
            Capability {
                cptr: dest_slot.offset,
                _cap_type: PhantomData,
            },
            dest_cnode,
        ))
    }
}

/////////
// TCB //
/////////
#[derive(Debug)]
pub struct ThreadControlBlock {}

impl CapType for ThreadControlBlock {
    type CopyOutput = Self;
    fn sel4_type_id() -> usize {
        api_object_seL4_TCBObject as usize
    }
}

impl FixedSizeCap for ThreadControlBlock {}

impl Capability<ThreadControlBlock> {
    pub fn configure<FreeSlots: Unsigned>(
        &mut self,
        // fault_ep: Capability<Endpoint>,
        cspace_root: CNode<FreeSlots, CNodeRoles::ChildProcess>,
        // cspace_root_data: usize, // set the guard bits here
        vspace_root: Capability<AssignedPageDirectory>, // TODO make a marker trait for VSpace?
                                                // vspace_root_data: usize, // always 0
                                                // buffer: usize,
                                                // buffer_frame: Capability<Frame>,
    ) -> Result<(), Error> {
        // Set up the cspace's guard to take the part of the cptr that's not
        // used by the radix.
        let cspace_root_data = unsafe {
            seL4_CNode_CapData_new(
                0,                                           // guard
                CONFIG_WORD_SIZE - cspace_root.radix as u32, // guard size in bits
            )
        }
        .words[0];

        let tcb_err = unsafe {
            seL4_TCB_Configure(
                self.cptr as u32,
                seL4_CapNull.into(), // fault_ep.cptr as u32,
                cspace_root.cptr as u32,
                cspace_root_data,
                vspace_root.cptr as u32,
                seL4_NilData.into(),
                0,
                0,
            )
        };

        if tcb_err != 0 {
            Err(Error::TCBConfigure(tcb_err))
        } else {
            Ok(())
        }
    }
}

// Others

#[derive(Debug)]
pub struct Endpoint {}

impl CapType for Endpoint {
    type CopyOutput = Self;
    fn sel4_type_id() -> usize {
        api_object_seL4_EndpointObject as usize
    }
}

impl FixedSizeCap for Endpoint {}

// asid control
#[derive(Debug)]
pub struct ASIDControl {}

impl CapType for ASIDControl {
    type CopyOutput = Self;
    fn sel4_type_id() -> usize {
        0 // TODO WUT
    }
}

impl Capability<ASIDControl> {
    // TODO this should happen in the bootstrap adapter
    pub fn wrap_cptr(cptr: usize) -> Capability<ASIDControl> {
        Capability {
            cptr: cptr,
            _cap_type: PhantomData,
        }
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
        0 // TODO WUT
    }
}

impl Capability<ASIDPool> {
    pub fn assign(&mut self, vspace: Capability<UnassignedPageDirectory>) -> Result<Capability<AssignedPageDirectory>, Error> {
        let err = unsafe { seL4_ARM_ASIDPool_Assign(self.cptr as u32, vspace.cptr as u32) };

        if err != 0 {
            return Err(Error::ASIDPoolAssign(err));
        }

        Ok(Capability {
            cptr: vspace.cptr,
            _cap_type: PhantomData
        })
    }
}

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

impl Capability<AssignedPageDirectory> {
    // TODO this should only happen in the bootstrap adapter
    pub fn wrap_cptr(cptr: usize) -> Capability<AssignedPageDirectory> {
        Capability {
            cptr: cptr,
            _cap_type: PhantomData,
        }
    }

    pub fn map_page_table(
        &mut self,
        page_table: Capability<UnmappedPageTable>,
        virtual_address: usize,
    ) -> Result<Capability<MappedPageTable>, Error> {
        // map the page table
        let err = unsafe {
            seL4_ARM_PageTable_Map(
                page_table.cptr as u32,
                self.cptr as u32,
                virtual_address as u32,
                // TODO: JON! What do we write here? The default (according to
                // sel4_ appears to be pageCachable | parityEnabled)
                seL4_ARM_VMAttributes_seL4_ARM_PageCacheable
                    | seL4_ARM_VMAttributes_seL4_ARM_ParityEnabled, // | seL4_ARM_VMAttributes_seL4_ARM_ExecuteNever
            )
        };

        if err != 0 {
            return Err(Error::MapPageTable(err));
        }
        Ok(Capability {
            cptr: page_table.cptr,
            _cap_type: PhantomData
        })
    }

    pub fn map_page(
        &mut self,
        page: Capability<UnmappedPage>,
        virtual_address: usize,
    ) -> Result<Capability<MappedPage>, Error> {
        let err = unsafe {
            seL4_ARM_Page_Map(
                page.cptr as u32,
                self.cptr as u32,
                virtual_address as u32,
                seL4_CapRights_new(0, 1, 1), // read/write
                // TODO: JON! What do we write here? The default (according to
                // sel4_ appears to be pageCachable | parityEnabled)
                seL4_ARM_VMAttributes_seL4_ARM_PageCacheable
                    | seL4_ARM_VMAttributes_seL4_ARM_ParityEnabled
                    // | seL4_ARM_VMAttributes_seL4_ARM_ExecuteNever,
            )
        };
        if err != 0 {
            return Err(Error::MapPage(err))
        }
        Ok(Capability {
            cptr: page.cptr,
            _cap_type: PhantomData
        })
    }
}

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

impl Capability<MappedPageTable> {
    pub fn unmap(self) -> Result<Capability<UnmappedPageTable>, Error> {
        let err = unsafe {seL4_ARM_PageTable_Unmap(self.cptr as u32) };
        if err != 0 {
            return Err(Error::UnmapPageTable(err));
        }
        Ok(Capability {
            cptr: self.cptr,
            _cap_type: PhantomData
        })
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

impl Capability<MappedPage> {
    pub fn wrap_cptr(cptr: usize) -> Capability<MappedPage> {
        Capability {
            cptr: cptr,
            _cap_type: PhantomData,
        }
    }

    pub fn unmap(self) -> Result<Capability<UnmappedPage>, Error>{
        let err = unsafe { seL4_ARM_Page_Unmap(self.cptr as u32) };
        if err != 0 {
            return Err(Error::UnmapPage(err));
        }
        Ok(Capability {
            cptr: self.cptr,
            _cap_type: PhantomData
        })
    }
}
