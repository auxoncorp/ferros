use core::marker::PhantomData;
use core::ops::{Add, Mul, Sub};

use selfe_sys::*;

use typenum::operator_aliases::{Diff, Prod, Sum};

use typenum::*;

use crate::arch::paging;
use crate::cap::{
    role, CNodeRole, Cap, CapRange, CapType, Delible, DirectRetype, LocalCap, Movable, PhantomCap,
};
use crate::error::SeL4Error;
use crate::pow::{Pow, _Pow};
use crate::userland::{
    CNode, CNodeSlot, CNodeSlots, ChildCNode, ChildCNodeSlots, LocalCNode, LocalCNodeSlot,
    LocalCNodeSlots, UnmappedPage,
};

// The seL4 kernel's maximum amount of retypes per system call is configurable
// in the sel4.toml, particularly by the KernelRetypeFanOutLimit property.
// This configuration is turned into a generated Rust type of the same name
// that implements `typenum::Unsigned` in the `build.rs` file.
include!(concat!(env!("OUT_DIR"), "/KERNEL_RETYPE_FAN_OUT_LIMIT"));

#[derive(Debug)]
pub struct Untyped<BitSize: Unsigned, Kind: MemoryKind = memory_kind::General> {
    pub(crate) _bit_size: PhantomData<BitSize>,
    pub(crate) _kind: PhantomData<Kind>,
}

impl<BitSize: Unsigned, Kind: MemoryKind> CapType for Untyped<BitSize, Kind> {}

impl<BitSize: Unsigned, Kind: MemoryKind> PhantomCap for Untyped<BitSize, Kind> {
    fn phantom_instance() -> Self {
        Untyped::<BitSize, Kind> {
            _bit_size: PhantomData::<BitSize>,
            _kind: PhantomData::<Kind>,
        }
    }
}

impl<BitSize: Unsigned, Kind: MemoryKind> Movable for Untyped<BitSize, Kind> {}

impl<BitSize: Unsigned, Kind: MemoryKind> Delible for Untyped<BitSize, Kind> {}

pub(crate) trait MemoryKind {}

pub mod memory_kind {
    use super::MemoryKind;

    #[derive(Debug, PartialEq)]
    pub struct General;
    impl MemoryKind for General {}

    #[derive(Debug, PartialEq)]
    pub struct Device;
    impl MemoryKind for Device {}
}

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
        dest_slots: LocalCNodeSlots<U2>,
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
        let (dest_cptr, dest_offset, _) = dest_slots.elim();

        let err = unsafe {
            seL4_Untyped_Retype(
                self.cptr,                              // _service
                api_object_seL4_UntypedObject as usize, // type
                BitSize::to_usize() - 1,                // size_bits
                dest_cptr,                              // root
                0,                                      // index
                0,                                      // depth
                dest_offset,                            // offset
                2,                                      // num_objects
            )
        };
        if err != 0 {
            return Err(SeL4Error::UntypedRetype(err));
        }

        Ok((
            Cap {
                cptr: dest_offset,
                cap_data: PhantomCap::phantom_instance(),
                _role: PhantomData,
            },
            Cap {
                cptr: dest_offset + 1,
                cap_data: PhantomCap::phantom_instance(),
                _role: PhantomData,
            },
        ))
    }

    pub fn quarter(
        self,
        dest_slots: LocalCNodeSlots<U4>,
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
        let (dest_cptr, dest_offset, _) = dest_slots.elim();
        let err = unsafe {
            seL4_Untyped_Retype(
                self.cptr,                              // _service
                api_object_seL4_UntypedObject as usize, // type
                BitSize::to_usize() - 2,                // size_bits
                dest_cptr,                              // root
                0,                                      // index
                0,                                      // depth
                dest_offset,                            // offset
                4,                                      // num_objects
            )
        };
        if err != 0 {
            return Err(SeL4Error::UntypedRetype(err));
        }

        Ok((
            Cap {
                cptr: dest_offset,
                cap_data: PhantomCap::phantom_instance(),
                _role: PhantomData,
            },
            Cap {
                cptr: dest_offset + 1,
                cap_data: PhantomCap::phantom_instance(),
                _role: PhantomData,
            },
            Cap {
                cptr: dest_offset + 2,
                cap_data: PhantomCap::phantom_instance(),
                _role: PhantomData,
            },
            Cap {
                cptr: dest_offset + 3,
                cap_data: PhantomCap::phantom_instance(),
                _role: PhantomData,
            },
        ))
    }

    /// Gain temporary access to an untyped capability for use in a function context.
    /// When the passed function call is complete, all capabilities derived
    /// from this untyped will be revoked (and thus destroyed).
    ///
    /// Be cautious not to return or store any capabilities created in this function, even in the error case.
    pub fn with_temporary<E, F>(
        &mut self,
        parent_cnode: &LocalCap<LocalCNode>,
        f: F,
    ) -> Result<Result<(), E>, SeL4Error>
    where
        F: FnOnce(Self) -> Result<(), E>,
    {
        // Call the function with an alias/copy of self
        let r = f(Cap {
            cptr: self.cptr,
            cap_data: Untyped {
                _bit_size: PhantomData,
                _kind: PhantomData,
            },
            _role: PhantomData,
        });

        // Clean up any child/derived capabilities that may have been created.
        let err = unsafe {
            seL4_CNode_Revoke(
                parent_cnode.cptr,   // _service
                self.cptr,           // index
                seL4_WordBits as u8, // depth
            )
        };
        if err != 0 {
            Err(SeL4Error::CNodeRevoke(err))
        } else {
            Ok(r)
        }
    }
}

/// A version of retype that concretely specifies the required untyped size,
/// to work well with type inference.
pub fn retype<TargetCapType: CapType, TargetRole: CNodeRole>(
    untyped: LocalCap<Untyped<TargetCapType::SizeBits, memory_kind::General>>,
    dest_slot: CNodeSlot<TargetRole>,
) -> Result<Cap<TargetCapType, TargetRole>, SeL4Error>
where
    TargetCapType: DirectRetype,
    TargetCapType: PhantomCap,
    TargetCapType::SizeBits: IsGreaterOrEqual<TargetCapType::SizeBits, Output = True>,
{
    untyped.retype(dest_slot)
}

/// A version of retype_cnode that concretely specifies the required untyped size,
/// to work well with type inference.
pub fn retype_cnode<ChildRadix: Unsigned>(
    untyped: LocalCap<Untyped<Sum<ChildRadix, U4>, memory_kind::General>>,
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
    Sum<ChildRadix, U4>: IsGreaterOrEqual<Sum<ChildRadix, U4>>,
{
    untyped.retype_cnode::<ChildRadix>(local_slots)
}

impl<BitSize: Unsigned> LocalCap<Untyped<BitSize, memory_kind::General>> {
    pub fn retype<TargetCapType: CapType, TargetRole: CNodeRole>(
        self,
        dest_slot: CNodeSlot<TargetRole>,
    ) -> Result<Cap<TargetCapType, TargetRole>, SeL4Error>
    where
        TargetCapType: DirectRetype,
        TargetCapType: PhantomCap,
        BitSize: IsGreaterOrEqual<TargetCapType::SizeBits, Output = True>,
    {
        let (dest_cptr, dest_offset, _) = dest_slot.elim();

        let err = unsafe {
            seL4_Untyped_Retype(
                self.cptr,                     // _service
                TargetCapType::sel4_type_id(), // type
                0,                             // size_bits
                dest_cptr,                     // root
                0,                             // index
                0,                             // depth
                dest_offset,                   // offset
                1,                             // num_objects
            )
        };

        if err != 0 {
            return Err(SeL4Error::UntypedRetype(err));
        }

        Ok(Cap {
            cptr: dest_offset,
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
        let (dest_cptr, dest_offset, _) = dest_slots.elim();
        let err = unsafe {
            seL4_Untyped_Retype(
                self.cptr,                     // _service
                TargetCapType::sel4_type_id(), // type
                0,                             // size_bits
                dest_cptr,                     // root
                0,                             // index
                0,                             // depth
                dest_offset,                   // offset
                Count::USIZE,                  // num_objects
            )
        };

        if err != 0 {
            return Err(SeL4Error::UntypedRetype(err));
        }

        Ok(CapRange {
            start_cptr: dest_offset,
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

        let (scratch_cptr, scratch_offset, _) = scratch_slot.elim();
        let (dest_cptr, dest_offset, _) = dest_slot.elim();

        unsafe {
            // Retype to fill the scratch slot with a fresh CNode
            let err = seL4_Untyped_Retype(
                self.cptr,                               // _service
                api_object_seL4_CapTableObject as usize, // type
                ChildRadix::to_usize(),                  // size_bits
                scratch_cptr,                            // root
                0,                                       // index
                0,                                       // depth
                scratch_offset,                          // offset
                1,                                       // num_objects
            );

            if err != 0 {
                return Err(SeL4Error::CNodeMutate(err));
            }

            // In order to set the guard (for the sake of our C-pointer simplification scheme),
            // mutate the CNode in the scratch slot, which copies the CNode into a second slot
            let guard_data = seL4_CNode_CapData_new(
                0,                                                      // guard
                (seL4_WordBits - ChildRadix::to_usize() as usize) as _, // guard size in bits
            )
            .words[0];

            let err = seL4_CNode_Mutate(
                dest_cptr,           // _service: seL4_CNode,
                dest_offset,         // dest_index: seL4_Word,
                seL4_WordBits as u8, // dest_depth: seL4_Uint8,
                scratch_cptr,        // src_root: seL4_CNode,
                scratch_offset,      // src_index: seL4_Word,
                seL4_WordBits as u8, // src_depth: seL4_Uint8,
                guard_data as usize, // badge or guard: seL4_Word,
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
                cptr: dest_offset,
                _role: PhantomData,
                cap_data: CNode {
                    radix: ChildRadix::to_u8(),
                    _role: PhantomData,
                },
            },
            // We start with the next free slot at 1 in order to "reserve" the 0-indexed slot for "null"
            CNodeSlots::internal_new(dest_offset, 1),
        ))
    }
}

impl LocalCap<Untyped<paging::PageBits, memory_kind::Device>> {
    /// The only thing memory_kind::Device memory can be used to make
    /// is a page/frame.
    pub fn retype_device_page(
        self,
        dest_slot: LocalCNodeSlot,
    ) -> Result<LocalCap<UnmappedPage<memory_kind::Device>>, SeL4Error> {
        // Note that we special case introduce a device page creation function
        // because the most likely alternative would be complicating the DirectRetype
        // trait to allow some sort of associated-type matching between the allowable
        // source Untyped memory kinds and the output cap types.

        let (dest_cptr, dest_offset, _) = dest_slot.elim();

        let err = unsafe {
            seL4_Untyped_Retype(
                self.cptr,                    // _service
                UnmappedPage::sel4_type_id(), // type
                0,                            // size_bits
                dest_cptr,                    // root
                0,                            // index
                0,                            // depth
                dest_offset,                  // offset
                1,                            // num_objects
            )
        };

        if err != 0 {
            return Err(SeL4Error::UntypedRetype(err));
        }

        Ok(Cap {
            cptr: dest_offset,
            cap_data: PhantomCap::phantom_instance(),
            _role: PhantomData,
        })
    }
}
