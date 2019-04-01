use core::marker::PhantomData;
use core::ops::{Add, Mul, Sub};
use crate::pow::{Pow, _Pow};
use crate::userland::{
    memory_kind, paging, role, CNode, CNodeRole, CNodeSlots, Cap, CapRange, CapType, ChildCNode,
    ChildCNodeSlots, ChildCap, DirectRetype, LocalCNodeSlot, LocalCNodeSlots, LocalCap, MemoryKind,
    NewCNodeSlot, PhantomCap, SeL4Error, UnmappedPage, Untyped,
};
use sel4_sys::*;
use typenum::operator_aliases::{Diff, Prod, Sub1, Sum};
use typenum::*;

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
    pub fn split(
        self,
        dest_slot: LocalCNodeSlot,
    ) -> Result<
        (
            LocalCap<Untyped<Diff<BitSize, U1>, Kind>>,
            LocalCap<Untyped<Diff<BitSize, U1>, Kind>>,
        ),
        SeL4Error,
    >
    where
        BitSize: Sub<U1>,
        Diff<BitSize, U1>: Unsigned,
    {
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
        ))
    }

    pub fn quarter(
        self,
        dest_slots: LocalCNodeSlots<U3>,
    ) -> Result<
        (
            LocalCap<Untyped<Diff<BitSize, U2>, Kind>>,
            LocalCap<Untyped<Diff<BitSize, U2>, Kind>>,
            LocalCap<Untyped<Diff<BitSize, U2>, Kind>>,
            LocalCap<Untyped<Diff<BitSize, U2>, Kind>>,
        ),
        SeL4Error,
    >
    where
        BitSize: Sub<U2>,
        Diff<BitSize, U2>: Unsigned,
    {
        let offset = dest_slots.offset;
        let err = unsafe {
            seL4_Untyped_Retype(
                self.cptr,                              // _service
                api_object_seL4_UntypedObject as usize, // type
                BitSize::to_usize() - 2,                // size_bits
                dest_slots.cptr,                        // root
                0,                                      // index
                0,                                      // depth
                offset,                                 // offset
                3,                                      // num_objects
            )
        };
        if err != 0 {
            return Err(SeL4Error::UntypedRetype(err));
        }

        Ok((
            Cap {
                cptr: offset,
                cap_data: PhantomCap::phantom_instance(),
                _role: PhantomData,
            },
            Cap {
                cptr: offset + 1,
                cap_data: PhantomCap::phantom_instance(),
                _role: PhantomData,
            },
            Cap {
                cptr: offset + 2,
                cap_data: PhantomCap::phantom_instance(),
                _role: PhantomData,
            },
            Cap {
                cptr: self.cptr,
                cap_data: PhantomCap::phantom_instance(),
                _role: PhantomData,
            },
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

    pub fn retype_multi<TargetCapType: CapType, Count: Unsigned>(
        self,
        dest_slots: LocalCNodeSlots<Count>,
    ) -> Result<CapRange<TargetCapType, role::Local, Count>, SeL4Error>
    where
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
        let err = unsafe {
            seL4_Untyped_Retype(
                self.cptr,                     // _service
                TargetCapType::sel4_type_id(), // type
                0,                             // size_bits
                dest_slots.cptr,               // root
                0,                             // index
                0,                             // depth
                dest_slots.offset,             // offset
                Count::USIZE,                  // num_objects
            )
        };

        if err != 0 {
            return Err(SeL4Error::UntypedRetype(err));
        }

        Ok(CapRange {
            start_cptr: dest_slots.offset,
            _cap_type: PhantomData,
            _role: PhantomData,
            _slots: PhantomData,
        })
    }

    pub fn retype_cnode<ChildRadix: Unsigned>(
        self,
        local_slots: LocalCNodeSlots<U2>,
    ) -> Result<
        (
            LocalCap<ChildCNode>,
            ChildCNodeSlots<Diff<Pow<ChildRadix>, U1>>,
        ),
        SeL4Error,
    >
    where
        ChildRadix: _Pow,
        Pow<ChildRadix>: Unsigned,

        Pow<ChildRadix>: Sub<U1>,
        Diff<Pow<ChildRadix>, U1>: Unsigned,

        ChildRadix: Add<U4>,
        Sum<ChildRadix, U4>: Unsigned,
        BitSize: IsGreaterOrEqual<Sum<ChildRadix, U4>>,
    {
        let (scratch_slot, local_slots) = local_slots.alloc::<U1>();
        let (dest_slot, _) = local_slots.alloc::<U1>();

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
                    _role: PhantomData,
                },
            },
            CNodeSlots {
                cptr: dest_slot.offset,
                // We start with the next free slot at 1 in order to "reserve" the 0-indexed slot for "null"
                offset: 1,
                _role: PhantomData,
                _size: PhantomData,
            },
        ))
    }
}

impl LocalCap<Untyped<paging::PageBits, memory_kind::Device>> {
    /// The only thing memory_kind::Device memory can be used to make
    /// is a page/frame.
    pub fn retype_device_page<CNodeFreeSlots: Unsigned>(
        self,
        dest_slot: LocalCNodeSlot,
    ) -> Result<LocalCap<UnmappedPage<memory_kind::Device>>, SeL4Error> {
        // Note that we special case introduce a device page creation function
        // because the most likely alternative would be complicating the DirectRetype
        // trait to allow some sort of associated-type matching between the allowable
        // source Untyped memory kinds and the output cap types.

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

        Ok(Cap {
            cptr: dest_slot.offset,
            cap_data: PhantomCap::phantom_instance(),
            _role: PhantomData,
        })
    }
}
