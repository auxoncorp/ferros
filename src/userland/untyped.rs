use core::marker::PhantomData;
use core::ops::{Add, Mul, Sub};
use crate::pow::{Pow, _Pow};
use crate::userland::{
    memory_kind, paging, role, CNode, CNodeRole, Cap, CapRange, CapType, ChildCNode, ChildCap,
    DirectRetype, LocalCap, MemoryKind, NewCNodeSlot, PhantomCap, SeL4Error, UnmappedPage, Untyped,
};
use sel4_sys::*;
use typenum::operator_aliases::{Diff, Prod, Sub1, Sum};
use typenum::{IsGreaterOrEqual, IsLessOrEqual, True, Unsigned, B1, U1, U12, U2, U3, U4};

// The seL4 kernel's maximum amount of retypes per system call is configurable
// in the fel4.toml, particularly by the KernelRetypeFanOutLimit property.
// This configuration is turned into a generated Rust type of the same name
// that implements `typenum::Unsigned` in the `build.rs` file.
include!(concat!(env!("OUT_DIR"), "/KERNEL_RETYPE_FAN_OUT_LIMIT"));

pub(crate) fn wrap_untyped<BitSize: Unsigned, Kind: MemoryKind>(
    cptr: usize,
    untyped_desc: &seL4_UntypedDesc,
) -> Option<LocalCap<Untyped<BitSize, Kind>>> {
    if untyped_desc.sizeBits == BitSize::to_u8() {
        Some(Cap {
            cptr,
            cap_data: PhantomCap::phantom_instance(),
            _role: PhantomData,
        })
    } else {
        None
    }
}

impl<BitSize: Unsigned, Kind: MemoryKind> LocalCap<Untyped<BitSize, Kind>> {
    pub fn split<FreeSlots: Unsigned>(
        self,
        dest_cnode: LocalCap<CNode<FreeSlots, role::Local>>,
    ) -> Result<
        (
            LocalCap<Untyped<Sub1<BitSize>, Kind>>,
            LocalCap<Untyped<Sub1<BitSize>, Kind>>,
            LocalCap<CNode<Sub1<FreeSlots>, role::Local>>,
        ),
        SeL4Error,
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
                self.cptr,                              // _service
                api_object_seL4_UntypedObject as usize, // type
                BitSize::to_usize() - 1,                // size_bits
                dest_slot.cptr,                         // root
                0,                                      // index
                0,                                      // depth
                dest_slot.offset,                       // offset
                1,                                      // num_objects
            )
        };
        if err != 0 {
            return Err(SeL4Error::UntypedRetype(err));
        }

        Ok((
            Cap {
                cptr: self.cptr,
                cap_data: PhantomCap::phantom_instance(),
                _role: PhantomData,
            },
            Cap {
                cptr: dest_slot.offset,
                cap_data: PhantomCap::phantom_instance(),
                _role: PhantomData,
            },
            dest_cnode,
        ))
    }

    pub fn quarter<FreeSlots: Unsigned>(
        self,
        dest_cnode: LocalCap<CNode<FreeSlots, role::Local>>,
    ) -> Result<
        (
            LocalCap<Untyped<Diff<BitSize, U2>, Kind>>,
            LocalCap<Untyped<Diff<BitSize, U2>, Kind>>,
            LocalCap<Untyped<Diff<BitSize, U2>, Kind>>,
            LocalCap<Untyped<Diff<BitSize, U2>, Kind>>,
            LocalCap<CNode<Sub1<Sub1<Sub1<FreeSlots>>>, role::Local>>,
        ),
        SeL4Error,
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
                self.cptr,                              // _service
                api_object_seL4_UntypedObject as usize, // type
                BitSize::to_usize() - 2,                // size_bits
                dest_slot1.cptr,                        // root
                0,                                      // index
                0,                                      // depth
                dest_slot1.offset,                      // offset
                3,                                      // num_objects
            )
        };
        if err != 0 {
            return Err(SeL4Error::UntypedRetype(err));
        }

        Ok((
            Cap {
                cptr: dest_slot1.offset,
                cap_data: PhantomCap::phantom_instance(),
                _role: PhantomData,
            },
            Cap {
                cptr: dest_slot2.offset,
                cap_data: PhantomCap::phantom_instance(),
                _role: PhantomData,
            },
            Cap {
                cptr: dest_slot3.offset,
                cap_data: PhantomCap::phantom_instance(),
                _role: PhantomData,
            },
            Cap {
                cptr: self.cptr,
                cap_data: PhantomCap::phantom_instance(),
                _role: PhantomData,
            },
            dest_cnode,
        ))
    }
}

impl<BitSize: Unsigned> LocalCap<Untyped<BitSize, memory_kind::General>> {
    pub fn retype<TargetCapType: CapType, TargetRole: CNodeRole>(
        self,
        dest_slot: NewCNodeSlot<TargetRole>,
    ) -> Result<Cap<TargetCapType, TargetRole>, SeL4Error>
    where
        TargetCapType: DirectRetype,
        TargetCapType: PhantomCap,
        BitSize: IsGreaterOrEqual<TargetCapType::SizeBits, Output = True>,
    {
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
            return Err(SeL4Error::UntypedRetype(err));
        }

        Ok(Cap {
            cptr: dest_slot.offset,
            cap_data: PhantomCap::phantom_instance(),
            _role: PhantomData,
        })
    }

    pub fn retype_local<FreeSlots: Unsigned, TargetCapType: CapType>(
        self,
        dest_cnode: LocalCap<CNode<FreeSlots, role::Local>>,
    ) -> Result<
        (
            LocalCap<TargetCapType>,
            LocalCap<CNode<Sub1<FreeSlots>, role::Local>>,
        ),
        SeL4Error,
    >
    where
        FreeSlots: Sub<B1>,
        Sub1<FreeSlots>: Unsigned,
        TargetCapType: DirectRetype,
        TargetCapType: PhantomCap,
        BitSize: IsGreaterOrEqual<TargetCapType::SizeBits, Output = True>,
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
            return Err(SeL4Error::UntypedRetype(err));
        }

        Ok((
            Cap {
                cptr: dest_slot.offset,
                cap_data: PhantomCap::phantom_instance(),
                _role: PhantomData,
            },
            dest_cnode,
        ))
    }

    pub fn retype_multi<FreeSlots: Unsigned, TargetCapType: CapType, Count: Unsigned>(
        self,
        dest_cnode: LocalCap<CNode<FreeSlots, role::Local>>,
    ) -> Result<
        (
            CapRange<TargetCapType, role::Local, Count>,
            LocalCap<CNode<Diff<FreeSlots, Count>, role::Local>>,
        ),
        SeL4Error,
    >
    where
        FreeSlots: Sub<Count>,
        Diff<FreeSlots, Count>: Unsigned,
        Count: IsLessOrEqual<KernelRetypeFanOutLimit, Output = True>,

        TargetCapType: DirectRetype,
        TargetCapType: PhantomCap,

        BitSize: _Pow,
        Pow<BitSize>: Unsigned,

        <TargetCapType as DirectRetype>::SizeBits: _Pow,
        Pow<<TargetCapType as DirectRetype>::SizeBits>: Mul<Count>,
        Prod<Pow<<TargetCapType as DirectRetype>::SizeBits>, Count>: Unsigned,

        Pow<BitSize>: IsGreaterOrEqual<
            Prod<Pow<<TargetCapType as DirectRetype>::SizeBits>, Count>,
            Output = True,
        >,
    {
        let (reservation, dest_cnode) = dest_cnode.reserve_region::<Count>();

        let err = unsafe {
            seL4_Untyped_Retype(
                self.cptr,                           // _service
                TargetCapType::sel4_type_id(),       // type
                0,                                   // size_bits
                reservation.cptr,                    // root
                0,                                   // index
                0,                                   // depth
                reservation.cap_data.next_free_slot, // offset
                Count::USIZE,                        // num_objects
            )
        };

        if err != 0 {
            return Err(SeL4Error::UntypedRetype(err));
        }

        Ok((
            CapRange {
                start_cptr: reservation.cap_data.next_free_slot,
                _cap_type: PhantomData,
                _role: PhantomData,
                _slots: PhantomData,
            },
            dest_cnode,
        ))
    }

    pub fn retype_cnode<FreeSlots: Unsigned, ChildRadix: Unsigned>(
        self,
        dest_cnode: LocalCap<CNode<FreeSlots, role::Local>>,
    ) -> Result<
        (
            LocalCap<CNode<Diff<Pow<ChildRadix>, U1>, role::Child>>,
            LocalCap<CNode<Diff<FreeSlots, U2>, role::Local>>,
        ),
        SeL4Error,
    >
    where
        FreeSlots: Sub<U2>,
        Diff<FreeSlots, U2>: Unsigned,
        ChildRadix: _Pow,
        Pow<ChildRadix>: Unsigned,

        Pow<ChildRadix>: Sub<U1>,
        Diff<Pow<ChildRadix>, U1>: Unsigned,

        ChildRadix: Add<U4>,
        Sum<ChildRadix, U4>: Unsigned,
        BitSize: IsGreaterOrEqual<Sum<ChildRadix, U4>>,
    {
        let (reserved_slots, output_dest_cnode) = dest_cnode.reserve_region::<U2>();
        let (reserved_slots, scratch_slot) = reserved_slots.consume_slot();
        let (_reserved_slots, dest_slot) = reserved_slots.consume_slot();

        unsafe {
            // Retype to fill the scratch slot with a fresh CNode
            let err = seL4_Untyped_Retype(
                self.cptr,                               // _service
                api_object_seL4_CapTableObject as usize, // type
                ChildRadix::to_usize(),                  // size_bits
                scratch_slot.cptr,                       // root
                0,                                       // index
                0,                                       // depth
                scratch_slot.offset,                     // offset
                1,                                       // num_objects
            );

            if err != 0 {
                return Err(SeL4Error::CNodeMutate(err));
            }

            // In order to set the guard (for the sake of our C-pointer simplification scheme),
            // mutate the CNode in the scratch slot, which copies the CNode into a second slot
            let guard_data = seL4_CNode_CapData_new(
                0,                                               // guard
                seL4_WordBits - ChildRadix::to_usize() as usize, // guard size in bits
            )
            .words[0];

            let err = seL4_CNode_Mutate(
                dest_slot.cptr,        // _service: seL4_CNode,
                dest_slot.offset,      // dest_index: seL4_Word,
                seL4_WordBits as u8,    // dest_depth: seL4_Uint8,
                scratch_slot.cptr,         // src_root: seL4_CNode,
                scratch_slot.offset,       // src_index: seL4_Word,
                seL4_WordBits as u8,    // src_depth: seL4_Uint8,
                guard_data              // badge or guard: seL4_Word,
            );

            // TODO - If we wanted to make more efficient use of our available slots at the cost
            // of complexity, we could swap the two created CNodes, then delete the one with
            // the incorrect guard (the one originally occupying the scratch slot).

            if err != 0 {
                return Err(SeL4Error::UntypedRetype(err));
            }
        }

        Ok((
            Cap {
                cptr: dest_slot.offset,
                _role: PhantomData,
                cap_data: CNode {
                    radix: ChildRadix::to_u8(),
                    // We start with the next free slot at 1 in order to "reserve" the 0-indexed slot for "null"
                    next_free_slot: 1,
                    _free_slots: PhantomData,
                    _role: PhantomData,
                },
            },
            output_dest_cnode,
        ))
    }

    pub fn retype_child<FreeSlots: Unsigned, TargetCapType: CapType>(
        self,
        dest_cnode: LocalCap<ChildCNode<FreeSlots>>,
    ) -> Result<
        (
            ChildCap<TargetCapType>,
            LocalCap<ChildCNode<Sub1<FreeSlots>>>,
        ),
        SeL4Error,
    >
    where
        FreeSlots: Sub<B1>,
        Sub1<FreeSlots>: Unsigned,
        TargetCapType: DirectRetype,
        TargetCapType: PhantomCap,
        BitSize: IsGreaterOrEqual<TargetCapType::SizeBits, Output = True>,
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
            return Err(SeL4Error::UntypedRetype(err));
        }

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

impl LocalCap<Untyped<paging::PageBits, memory_kind::Device>> {
    /// The only thing memory_kind::Device memory can be used to make
    /// is a page/frame.
    pub fn retype_device_page<CNodeFreeSlots: Unsigned>(
        self,
        dest_cnode: LocalCap<CNode<CNodeFreeSlots, role::Local>>,
    ) -> Result<
        (
            LocalCap<UnmappedPage<memory_kind::Device>>,
            LocalCap<CNode<Sub1<CNodeFreeSlots>, role::Local>>,
        ),
        SeL4Error,
    >
    where
        CNodeFreeSlots: Sub<B1>,
        Sub1<CNodeFreeSlots>: Unsigned,
    {
        // Note that we special case introduce a device page creation function
        // because the most likely alternative would be complicating the DirectRetype
        // trait to allow some sort of associated-type matching between the allowable
        // source Untyped memory kinds and the output cap types.
        let (dest_cnode, dest_slot) = dest_cnode.consume_slot();

        let err = unsafe {
            seL4_Untyped_Retype(
                self.cptr,                    // _service
                UnmappedPage::sel4_type_id(), // type
                0,                            // size_bits
                dest_slot.cptr,               // root
                0,                            // index
                0,                            // depth
                dest_slot.offset,             // offset
                1,                            // num_objects
            )
        };

        if err != 0 {
            return Err(SeL4Error::UntypedRetype(err));
        }

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
