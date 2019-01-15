use crate::pow::{Pow, _Pow};
use crate::userland::{role, CNode, Cap, CapType, Error, FixedSizeCap, Untyped};
use core::marker::PhantomData;
use core::ops::Sub;
use sel4_sys::*;
use typenum::operator_aliases::{Diff, Sub1};
use typenum::{Unsigned, B1, U2, U3};

pub fn wrap_untyped<BitSize: Unsigned>(
    cptr: usize,
    untyped_desc: &seL4_UntypedDesc,
) -> Option<Cap<Untyped<BitSize>, role::Local>> {
    if untyped_desc.sizeBits == BitSize::to_u8() {
        Some(Cap {
            cptr,
            _cap_type: PhantomData,
            _role: PhantomData,
        })
    } else {
        None
    }
}

impl<BitSize: Unsigned> Cap<Untyped<BitSize>, role::Local> {
    pub fn split<FreeSlots: Unsigned>(
        self,
        dest_cnode: CNode<FreeSlots, role::Local>,
    ) -> Result<
        (
            Cap<Untyped<Sub1<BitSize>>, role::Local>,
            Cap<Untyped<Sub1<BitSize>>, role::Local>,
            CNode<Sub1<FreeSlots>, role::Local>,
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
                self.cptr,                          // _service
                Untyped::<BitSize>::sel4_type_id(), // type
                (BitSize::to_usize() - 1),          // size_bits
                dest_slot.cptr,                     // root
                0,                                  // index
                0,                                  // depth
                dest_slot.offset,                   // offset
                1,                                  // num_objects
            )
        };
        if err != 0 {
            return Err(Error::UntypedRetype(err));
        }

        Ok((
            Cap {
                cptr: self.cptr,
                _cap_type: PhantomData,
                _role: PhantomData,
            },
            Cap {
                cptr: dest_slot.offset,
                _cap_type: PhantomData,
                _role: PhantomData,
            },
            dest_cnode,
        ))
    }

    pub fn quarter<FreeSlots: Unsigned>(
        self,
        dest_cnode: CNode<FreeSlots, role::Local>,
    ) -> Result<
        (
            Cap<Untyped<Diff<BitSize, U2>>, role::Local>,
            Cap<Untyped<Diff<BitSize, U2>>, role::Local>,
            Cap<Untyped<Diff<BitSize, U2>>, role::Local>,
            Cap<Untyped<Diff<BitSize, U2>>, role::Local>,
            CNode<Sub1<Sub1<Sub1<FreeSlots>>>, role::Local>,
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
                self.cptr,                          // _service
                Untyped::<BitSize>::sel4_type_id(), // type
                (BitSize::to_usize() - 2),          // size_bits
                dest_slot1.cptr,                    // root
                0,                                  // index
                0,                                  // depth
                dest_slot1.offset,                  // offset
                3,                                  // num_objects
            )
        };
        if err != 0 {
            return Err(Error::UntypedRetype(err));
        }

        Ok((
            Cap {
                cptr: self.cptr,
                _cap_type: PhantomData,
                _role: PhantomData,
            },
            Cap {
                cptr: dest_slot1.offset,
                _cap_type: PhantomData,
                _role: PhantomData,
            },
            Cap {
                cptr: dest_slot2.offset,
                _cap_type: PhantomData,
                _role: PhantomData,
            },
            Cap {
                cptr: dest_slot3.offset,
                _cap_type: PhantomData,
                _role: PhantomData,
            },
            dest_cnode,
        ))
    }

    // TODO add required bits as an associated type for each TargetCapType, require that
    // this untyped is big enough
    pub fn retype_local<FreeSlots: Unsigned, TargetCapType: CapType>(
        self,
        dest_cnode: CNode<FreeSlots, role::Local>,
    ) -> Result<
        (
            Cap<TargetCapType, role::Local>,
            CNode<Sub1<FreeSlots>, role::Local>,
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
                self.cptr,                     // _service
                TargetCapType::sel4_type_id(), // type
                0,                             // size_bits
                dest_slot.cptr,                // root
                0,                             // index
                0,                             // depth
                dest_slot.offset,              // offset
                1,                             // num_objects
            )
        };

        if err != 0 {
            return Err(Error::UntypedRetype(err));
        }

        Ok((
            Cap {
                cptr: dest_slot.offset,
                _cap_type: PhantomData,
                _role: PhantomData,
            },
            dest_cnode,
        ))
    }

    // TODO: the required size of the untyped depends in some way on the child radix, but HOW?
    // answer: it needs 4 more bits, this value is seL4_SlotBits.
    pub fn retype_local_cnode<FreeSlots: Unsigned, ChildRadix: Unsigned>(
        self,
        dest_cnode: CNode<FreeSlots, role::Local>,
    ) -> Result<
        (
            CNode<Pow<ChildRadix>, role::Child>,
            CNode<Sub1<FreeSlots>, role::Local>,
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
                self.cptr,                               // _service
                api_object_seL4_CapTableObject as usize, // type
                ChildRadix::to_usize(),                  // size_bits
                dest_slot.cptr,                          // root
                0,                                       // index
                0,                                       // depth
                dest_slot.offset,                        // offset
                1,                                       // num_objects
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
        dest_cnode: CNode<FreeSlots, role::Child>,
    ) -> Result<
        (
            Cap<TargetCapType, role::Child>,
            CNode<Sub1<FreeSlots>, role::Child>,
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
                self.cptr,                     // _service
                TargetCapType::sel4_type_id(), // type
                0,                             // size_bits
                dest_slot.cptr,                // root
                0,                             // index
                0,                             // depth
                dest_slot.offset,              // offset
                1,                             // num_objects
            )
        };

        if err != 0 {
            return Err(Error::UntypedRetype(err));
        }

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
