use core::marker::PhantomData;
use core::mem::size_of;
use core::ops::{Add, Sub};
use sel4_sys::{
    _object_seL4_ARM_PageDirectoryObject, _object_seL4_ARM_PageTableObject,
    _object_seL4_ARM_SmallPageObject, api_object_seL4_CapTableObject,
    api_object_seL4_EndpointObject, api_object_seL4_TCBObject, api_object_seL4_UntypedObject,
    seL4_ARM_ASIDControl_MakePool, seL4_ARM_ASIDPool, seL4_ARM_ASIDPool_Assign,
    seL4_ARM_PageTable_Map, seL4_ARM_Page_Map, seL4_ARM_VMAttributes_seL4_ARM_ExecuteNever,
    seL4_ARM_VMAttributes_seL4_ARM_PageCacheable, seL4_ARM_VMAttributes_seL4_ARM_ParityEnabled,
    seL4_BootInfo, seL4_CNode_CapData_new, seL4_CNode_Copy, seL4_CPtr, seL4_CapInitThreadCNode,
    seL4_CapNull, seL4_CapRights_new, seL4_CapRights_t, seL4_NilData, seL4_SlotRegion,
    seL4_TCB_Configure, seL4_UntypedDesc, seL4_Untyped_Retype, seL4_Word, seL4_WordBits,
};
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
    ASIDPoolAssign(u32),
    MapPage(u32),
    CNodeCopy(u32),
}

pub trait CapType {
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

#[derive(Debug)]
pub struct CNode<Radix: Unsigned, FreeSlots: Unsigned, Role: CNodeRole> {
    _radix: PhantomData<Radix>,
    _free_slots: PhantomData<FreeSlots>,
    _role: PhantomData<Role>,
}

impl<Radix: Unsigned, FreeSlots: Unsigned, Role: CNodeRole> CapType
    for CNode<Radix, FreeSlots, Role>
{
    fn sel4_type_id() -> usize {
        api_object_seL4_CapTableObject as usize
    }
}

#[derive(Debug)]
struct CNodeSlot {
    cptr: usize,
    offset: usize,
}

impl<Radix: Unsigned, FreeSlots: Unsigned, Role: CNodeRole>
    Capability<CNode<Radix, FreeSlots, Role>>
{
    // TODO: reverse these args to be consistent with everything else
    fn consume_slot(self) -> (Capability<CNode<Radix, Sub1<FreeSlots>, Role>>, CNodeSlot)
    where
        FreeSlots: Sub<B1>,
        Sub1<FreeSlots>: Unsigned,
    {
        (
            Capability {
                cptr: self.cptr,
                _cap_type: PhantomData,
            },
            CNodeSlot {
                cptr: self.cptr,
                offset: (1 << Radix::to_u8()) - FreeSlots::to_usize(),
            },
        )
    }

    // Reserve n slots that you're going to need for something outside this
    // api. Return the offset of the first slot and the updated cnode.
    pub fn reserve_region<Count: Unsigned>(
        self,
    ) -> (
        seL4_SlotRegion,
        Capability<CNode<Radix, Diff<FreeSlots, Count>, Role>>,
    )
    where
        FreeSlots: Sub<Count>,
        Diff<FreeSlots, Count>: Unsigned,
    {
        let start = ((1 << Radix::to_u8()) - FreeSlots::to_usize()) as u32;
        (
            seL4_SlotRegion {
                start: start,
                end: start + Count::to_u32(),
            },
            Capability {
                cptr: self.cptr,
                _cap_type: PhantomData,
            },
        )
    }
}

// TODO: how many slots are there really? We should be able to know this at build
// time.
// Answer: The radix is 19, and there are 12 initial caps.
// TODO: ideally, this should only be callable once in the process. Is that possible?
pub fn root_cnode(
    bootinfo: &'static seL4_BootInfo,
) -> Capability<CNode<U19, U1024, CNodeRoles::CSpaceRoot>> {
    Capability {
        cptr: seL4_CapInitThreadCNode as usize,
        _cap_type: PhantomData,
    }
}

impl<CT: CapType> Capability<CT> {
    pub fn copy_local<Radix: Unsigned, FreeSlots: Unsigned>(
        &self,
        dest_cnode: Capability<CNode<Radix, FreeSlots, CNodeRoles::CSpaceRoot>>,
        rights: seL4_CapRights_t,
    ) -> Result<
        (
            Capability<CT>,
            Capability<CNode<Radix, Sub1<FreeSlots>, CNodeRoles::CSpaceRoot>>,
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
                dest_slot.cptr as u32,          // _service
                dest_slot.offset as u32,        // index
                (8 * size_of::<usize>()) as u8, // depth
                // TODO this is hardcoded to the root task cnode
                2,                              // src_root
                self.cptr as u32,               // src_index
                32,                             // src_depth
                rights,                         // rights
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
    pub fn split<Radix: Unsigned, FreeSlots: Unsigned>(
        self,
        dest_cnode: Capability<CNode<Radix, FreeSlots, CNodeRoles::CSpaceRoot>>,
    ) -> Result<
        (
            Capability<Untyped<Sub1<BitSize>>>,
            Capability<Untyped<Sub1<BitSize>>>,
            Capability<CNode<Radix, Sub1<FreeSlots>, CNodeRoles::CSpaceRoot>>,
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

    pub fn quarter<Radix: Unsigned, FreeSlots: Unsigned>(
        self,
        dest_cnode: Capability<CNode<Radix, FreeSlots, CNodeRoles::CSpaceRoot>>,
    ) -> Result<
        (
            Capability<Untyped<Diff<BitSize, U2>>>,
            Capability<Untyped<Diff<BitSize, U2>>>,
            Capability<Untyped<Diff<BitSize, U2>>>,
            Capability<Untyped<Diff<BitSize, U2>>>,
            Capability<CNode<Radix, Sub1<Sub1<Sub1<FreeSlots>>>, CNodeRoles::CSpaceRoot>>,
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
    pub fn retype_local<Radix: Unsigned, FreeSlots: Unsigned, TargetCapType: CapType>(
        self,
        dest_cnode: Capability<CNode<Radix, FreeSlots, CNodeRoles::CSpaceRoot>>,
    ) -> Result<
        (
            Capability<TargetCapType>,
            Capability<CNode<Radix, Sub1<FreeSlots>, CNodeRoles::CSpaceRoot>>,
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
    pub fn retype_local_cnode<Radix: Unsigned, FreeSlots: Unsigned, ChildRadix: Unsigned>(
        self,
        dest_cnode: Capability<CNode<Radix, FreeSlots, CNodeRoles::CSpaceRoot>>,
    ) -> Result<
        (
            Capability<CNode<ChildRadix, Pow<ChildRadix>, CNodeRoles::ChildProcess>>,
            Capability<CNode<Radix, Sub1<FreeSlots>, CNodeRoles::CSpaceRoot>>,
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
            Capability {
                cptr: dest_slot.offset,
                _cap_type: PhantomData,
            },
            dest_cnode,
        ))
    }

    pub fn retype_child<Radix: Unsigned, FreeSlots: Unsigned, TargetCapType: CapType>(
        self,
        dest_cnode: Capability<CNode<Radix, FreeSlots, CNodeRoles::ChildProcess>>,
    ) -> Result<
        (
            ChildCapability<TargetCapType>,
            Capability<CNode<Radix, Sub1<FreeSlots>, CNodeRoles::ChildProcess>>,
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

impl Capability<Untyped<U12>> {
    // TODO put retype local into a trait so we can dispatch via the target cap type
    pub fn retype_asid_pool<Radix: Unsigned, FreeSlots: Unsigned>(
        self,
        asid_control: Capability<ASIDControl>,
        dest_cnode: Capability<CNode<Radix, FreeSlots, CNodeRoles::CSpaceRoot>>,
    ) -> Result<
        (
            Capability<ASIDPool>,
            Capability<CNode<Radix, Sub1<FreeSlots>, CNodeRoles::CSpaceRoot>>,
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
    fn sel4_type_id() -> usize {
        api_object_seL4_TCBObject as usize
    }
}

impl FixedSizeCap for ThreadControlBlock {}

impl Capability<ThreadControlBlock> {
    pub fn configure<Radix: Unsigned, FreeSlots: Unsigned>(
        &mut self,
        // fault_ep: Capability<Endpoint>,
        cspace_root: Capability<CNode<Radix, FreeSlots, CNodeRoles::ChildProcess>>,
        // cspace_root_data: usize, // set the guard bits here
        vspace_root: Capability<PageDirectory>, // TODO make a marker trait for VSpace?
                                                // vspace_root_data: usize, // always 0
                                                // buffer: usize,
                                                // buffer_frame: Capability<Frame>,
    ) -> Result<(), Error> {
        // Set up the cspace's guard to take the part of the cptr that's not
        // used by the radix.
        let cspace_root_data = unsafe {
            seL4_CNode_CapData_new(
                0,                                                     // guard
                ((8 * size_of::<usize>()) - Radix::to_usize()) as u32, // guard size in bits
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
    fn sel4_type_id() -> usize {
        api_object_seL4_EndpointObject as usize
    }
}

impl FixedSizeCap for Endpoint {}

// asid control
#[derive(Debug)]
pub struct ASIDControl {}

impl CapType for ASIDControl {
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
    fn sel4_type_id() -> usize {
        0 // TODO WUT
    }
}

impl Capability<ASIDPool> {
    pub fn assign(&mut self, vspace: &mut Capability<PageDirectory>) -> Result<(), Error> {
        let err = unsafe { seL4_ARM_ASIDPool_Assign(self.cptr as u32, vspace.cptr as u32) };

        if err != 0 {
            Err(Error::ASIDPoolAssign(err))
        } else {
            Ok(())
        }
    }
}

#[derive(Debug)]
pub struct PageDirectory {}

impl CapType for PageDirectory {
    fn sel4_type_id() -> usize {
        _object_seL4_ARM_PageDirectoryObject as usize
    }
}

impl FixedSizeCap for PageDirectory {}

impl Capability<PageDirectory> {
    // TODO this should happen in the bootstrap adapter
    pub fn wrap_cptr(cptr: usize) -> Capability<PageDirectory> {
        Capability {
            cptr: cptr,
            _cap_type: PhantomData,
        }
    }

    pub fn map_page_table(
        &mut self,
        page_table: &Capability<PageTable>,
        virtual_address: usize,
    ) -> Result<(), Error> {
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
            Err(Error::MapPageTable(err))
        } else {
            Ok(())
        }
    }

    pub fn map_page(
        &mut self,
        page: &Capability<Page>,
        virtual_address: usize,
    ) -> Result<(), Error> {
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
            Err(Error::MapPage(err))
        } else {
            Ok(())
        }
    }
}

#[derive(Debug)]
pub struct PageTable {}

impl CapType for PageTable {
    fn sel4_type_id() -> usize {
        _object_seL4_ARM_PageTableObject as usize
    }
}

impl FixedSizeCap for PageTable {}

impl Capability<PageTable> {
    pub fn wrap_cptr(cptr: usize) -> Capability<PageTable> {
        Capability {
            cptr: cptr,
            _cap_type: PhantomData,
        }
    }
}

#[derive(Debug)]
pub struct Page {}

impl CapType for Page {
    fn sel4_type_id() -> usize {
        _object_seL4_ARM_SmallPageObject as usize
    }
}

impl FixedSizeCap for Page {}

impl Capability<Page> {
    pub fn wrap_cptr(cptr: usize) -> Capability<Page> {
        Capability {
            cptr: cptr,
            _cap_type: PhantomData,
        }
    }
}
