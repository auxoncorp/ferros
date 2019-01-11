use core::marker::PhantomData;
use core::ops::{Add, Sub};
use sel4_sys::{
    api_object_seL4_TCBObject, api_object_seL4_UntypedObject, seL4_BootInfo, seL4_CPtr,
    seL4_CapInitThreadCNode, seL4_UntypedDesc, seL4_Untyped_Retype, seL4_Word, seL4_WordBits,
};
use typenum::operator_aliases::{Add1, Diff, Shleft, Sub1};
use typenum::{
    Bit, Exp, IsGreaterOrEqual, UInt, UTerm, Unsigned, B0, B1, U1, U12, U2, U24, U256, U3, U32, U5,
    U8,
};

#[derive(Debug)]
pub struct CNode<Radix: Unsigned, FreeSlots: Unsigned> {
    cptr: seL4_CPtr,
    depth: usize,
    index: usize,

    offset: usize,

    _radix: PhantomData<Radix>,
    _free_slots: PhantomData<FreeSlots>,
}

#[derive(Debug)]
pub struct CNodeSlot {
    cptr: seL4_CPtr,
    depth: usize,
    index: usize,
    offset: usize,
}

impl<Radix: Unsigned, FreeSlots: Unsigned> CNode<Radix, FreeSlots>
where
    FreeSlots: Sub<B1>,
    Sub1<FreeSlots>: Unsigned,
{
    // TODO: this should return the tuple of (CNode, consumed slot)
    // this probably means we need a Slot struct.
    fn consume_slot(self) -> (CNode<Radix, Sub1<FreeSlots>>, CNodeSlot) {
        (
            // TODO: do this with transmute
            CNode {
                cptr: self.cptr,
                index: self.index,
                depth: self.depth,
                offset: self.offset + 1,
                _radix: PhantomData,
                _free_slots: PhantomData,
            },
            CNodeSlot {
                cptr: self.cptr,
                index: self.index,
                depth: self.depth,
                offset: self.offset,
            },
        )
    }
}

// TODO: how many slots are there really? We should be able to know this at build
// time.
// TODO: ideally, this should only be callable once in the process. Is that possible?
pub fn root_cnode(bootinfo: &'static seL4_BootInfo) -> CNode<U12, U256> {
    CNode {
        cptr: seL4_CapInitThreadCNode,
        index: seL4_WordBits as usize,
        depth: 0,
        offset: bootinfo.empty.start as usize,
        _radix: PhantomData,
        _free_slots: PhantomData,
    }
}

/////
// Capability types
pub trait CapType {
    fn sel4_type_id() -> usize;
}

#[derive(Debug)]
pub struct Capability<CT: CapType> {
    cptr: usize,
    _cap_type: PhantomData<CT>,
}

// Untyped
#[derive(Debug)]
pub struct Untyped<BitSize: Unsigned> {
    _bit_size: PhantomData<BitSize>,
}

impl<BitSize: Unsigned> CapType for Untyped<BitSize> {
    fn sel4_type_id() -> usize {
        api_object_seL4_UntypedObject as usize
    }
}

// TCB
#[derive(Debug)]
pub struct ThreadControlBlock {}

impl CapType for ThreadControlBlock {
    fn sel4_type_id() -> usize {
        api_object_seL4_TCBObject as usize
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

#[derive(Debug)]
pub enum Error {
    UntypedRetype(u32),
}

pub trait Split<Radix: Unsigned, FreeSlots: Unsigned>
where
    FreeSlots: Sub<B1>,
    Sub1<FreeSlots>: Unsigned,
    <Self as Split<Radix, FreeSlots>>::OutputBitSize: Unsigned,
{
    type OutputBitSize;

    fn split(
        self,
        dest_cnode: CNode<Radix, FreeSlots>,
    ) -> Result<
        (
            Capability<Untyped<Self::OutputBitSize>>,
            Capability<Untyped<Self::OutputBitSize>>,
            CNode<Radix, Sub1<FreeSlots>>,
        ),
        Error,
    >;
}

impl<Radix: Unsigned, FreeSlots: Unsigned, BitSize: Unsigned> Split<Radix, FreeSlots>
    for Capability<Untyped<BitSize>>
where
    FreeSlots: Sub<B1>,
    Sub1<FreeSlots>: Unsigned,

    BitSize: Sub<B1>,
    Sub1<BitSize>: Unsigned,
{
    type OutputBitSize = Sub1<BitSize>;

    fn split(
        self,
        dest_cnode: CNode<Radix, FreeSlots>,
    ) -> Result<
        (
            Capability<Untyped<Self::OutputBitSize>>,
            Capability<Untyped<Self::OutputBitSize>>,
            CNode<Radix, Sub1<FreeSlots>>,
        ),
        Error,
    > {
        let (dest_cnode, dest_slot) = dest_cnode.consume_slot();

        let err = unsafe {
            seL4_Untyped_Retype(
                self.cptr as u32,                          // _service
                Untyped::<BitSize>::sel4_type_id() as u32, // type
                Self::OutputBitSize::to_u32(),             // size_bits
                dest_slot.cptr,                            // root
                dest_slot.index as u32,                    // index
                dest_slot.depth as u32,                    // depth
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
                cptr: if dest_slot.depth == 0 {
                    dest_slot.offset
                } else {
                    unimplemented!()
                },

                _cap_type: PhantomData,
            },
            dest_cnode,
        ))
    }
}

//pub trait Retype

pub trait Retype<Radix: Unsigned, FreeSlots: Unsigned, TargetCapType: CapType>
where
    FreeSlots: Sub<B1>,
    Sub1<FreeSlots>: Unsigned,
{
    fn retype(
        self,
        dest_cnode: CNode<Radix, FreeSlots>,
    ) -> Result<(Capability<TargetCapType>, CNode<Radix, Sub1<FreeSlots>>), Error>;
}

impl<Radix: Unsigned, FreeSlots: Unsigned, TargetCapType: CapType, BitSize: Unsigned>
    Retype<Radix, FreeSlots, TargetCapType> for Capability<Untyped<BitSize>>
where
    FreeSlots: Sub<B1>,
    Sub1<FreeSlots>: Unsigned,
    // TODO: make sure the untyped has enough room for the target type
{
    fn retype(
        self,
        dest_cnode: CNode<Radix, FreeSlots>,
    ) -> Result<(Capability<TargetCapType>, CNode<Radix, Sub1<FreeSlots>>), Error> {
        let (dest_cnode, dest_slot) = dest_cnode.consume_slot();

        let err = unsafe {
            seL4_Untyped_Retype(
                self.cptr as u32,                     // _service
                TargetCapType::sel4_type_id() as u32, // type
                0,                                    // size_bits
                dest_slot.cptr,                       // root
                dest_slot.index as u32,               // index
                dest_slot.depth as u32,               // depth
                dest_slot.offset as u32,              // offset
                1,                                    // num_objects
            )
        };
        if err != 0 {
            return Err(Error::UntypedRetype(err));
        }

        Ok((
            Capability {
                cptr: if dest_slot.depth == 0 {
                    dest_slot.offset
                } else {
                    unimplemented!()
                },

                _cap_type: PhantomData,
            },
            dest_cnode,
        ))
    }
}
