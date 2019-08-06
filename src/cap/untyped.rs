use core::marker::PhantomData;
use core::ops::{Add, Mul, Sub};

use selfe_sys::*;

use typenum::operator_aliases::{Diff, Prod, Sum};

use typenum::*;

use crate::arch::{self, CNodeSlotBits, PageBits};
use crate::cap::{
    granule_state, role, CNode, CNodeRole, CNodeSlot, CNodeSlots, CNodeSlotsError, Cap, CapRange,
    CapType, ChildCNode, ChildCNodeSlots, Delible, DirectRetype, Granule, GranuleSlotCount,
    LocalCNode, LocalCNodeSlot, LocalCNodeSlots, LocalCap, Movable, Page, PhantomCap, WCNodeSlots,
    WCNodeSlotsData, WeakCapRange,
};
use crate::error::{ErrorExt, KernelError, SeL4Error};
use crate::pow::{Pow, _Pow};

// The seL4 kernel's maximum amount of retypes per system call is configurable
// in the sel4.toml, particularly by the KernelRetypeFanOutLimit property.
// This configuration is turned into a generated Rust type of the same name
// that implements `typenum::Unsigned` in the `build.rs` file.
include!(concat!(env!("OUT_DIR"), "/KERNEL_RETYPE_FAN_OUT_LIMIT"));

#[derive(Debug)]
pub struct Untyped<BitSize: Unsigned, Kind: MemoryKind = memory_kind::General> {
    pub(crate) kind: Kind,
    pub(crate) _bit_size: PhantomData<BitSize>,
}

/// Weakly-typed (runtime-managed) Untyped
#[derive(Debug)]
pub struct WUntyped<Kind: MemoryKind> {
    pub(crate) kind: Kind,
    pub(crate) size_bits: u8,
}

impl<BitSize: Unsigned, Kind: MemoryKind> CapType for Untyped<BitSize, Kind> {}

impl<Kind: MemoryKind> CapType for WUntyped<Kind> {}

impl<Kind: MemoryKind> LocalCap<WUntyped<Kind>> {
    pub fn size_bits(&self) -> u8 {
        self.cap_data.size_bits
    }

    pub fn size_bytes(&self) -> usize {
        2_usize.pow(self.cap_data.size_bits as u32)
    }

    pub fn as_strong<SizeBits: Unsigned>(self) -> Option<LocalCap<Untyped<SizeBits, Kind>>> {
        if self.size_bits() == SizeBits::U8 {
            return Some(Cap {
                cptr: self.cptr,
                cap_data: Untyped {
                    _bit_size: PhantomData,
                    kind: self.cap_data.kind,
                },
                _role: PhantomData,
            });
        }
        None
    }
    pub fn split(
        self,
        dest_slots: LocalCNodeSlots<U2>,
    ) -> Result<(LocalCap<WUntyped<Kind>>, LocalCap<WUntyped<Kind>>), WUntypedSplitError> {
        let output_size_bits = self.cap_data.size_bits - 1;
        if output_size_bits < crate::arch::PageBits::U8 {
            return Err(WUntypedSplitError::TooSmallToBeSplit);
        }

        let (dest_cptr, dest_offset, _) = dest_slots.elim();

        unsafe {
            seL4_Untyped_Retype(
                self.cptr,                              // _service
                api_object_seL4_UntypedObject as usize, // type
                usize::from(output_size_bits),          // size_bits
                dest_cptr,                              // root
                0,                                      // index
                0,                                      // depth
                dest_offset,                            // offset
                2,                                      // num_objects
            )
        }
        .as_result()
        .map_err(|e| WUntypedSplitError::UntypedRetypeError(e))?;

        let (kind_a, kind_b) = self
            .cap_data
            .kind
            .halve(2usize.pow(u32::from(self.cap_data.size_bits)))
            .ok_or_else(|| WUntypedSplitError::MemoryRegionWouldExceedAddressableSpace)?;
        Ok((
            Cap {
                cptr: dest_offset,
                cap_data: WUntyped {
                    kind: kind_a,
                    size_bits: output_size_bits,
                },
                _role: PhantomData,
            },
            Cap {
                cptr: dest_offset + 1,
                cap_data: WUntyped {
                    kind: kind_b,
                    size_bits: output_size_bits,
                },
                _role: PhantomData,
            },
        ))
    }

    pub fn retype_memory<CRole: CNodeRole>(
        self,
        slots: &mut Cap<WCNodeSlotsData<CRole>, role::Local>,
    ) -> Result<WeakCapRange<Granule<granule_state::Unmapped>, CRole>, RetypeError> {
        if self.cap_data.size_bits < PageBits::U8 {
            return Err(RetypeError::NotBigEnough);
        }
        let gran_info = arch::determine_best_granule_fit(self.cap_data.size_bits);
        if gran_info.count > KernelRetypeFanOutLimit::USIZE {
            // it probably isn't
            return Err(RetypeError::KernelRetypeFanOutLimit);
        }
        // TODO - REVIEW - Do we need more constraints on num_pages?
        let dest_slots = slots
            .alloc(gran_info.count)
            .map_err(|e| RetypeError::CNodeSlotsError(e))?;
        unsafe {
            seL4_Untyped_Retype(
                self.cptr,                  // _service
                gran_info.type_id,          // type
                0,                          // size_bits
                dest_slots.cptr,            // root
                0,                          // index
                0,                          // depth
                dest_slots.cap_data.offset, // offset
                gran_info.count,            // num_objects
            )
            .as_result()
            .map_err(|e| SeL4Error::UntypedRetype(e))?;
        }

        Ok(WeakCapRange::new(
            dest_slots.cap_data.offset,
            Page {
                state: granule_state::Unmapped,
                // TODO - kind piping
                //memory_kind: self.cap_data.kind,
            },
            gran_info.count,
        ))
    }
}

impl LocalCap<WUntyped<memory_kind::General>> {
    pub fn retype<D: CapType + PhantomCap + DirectRetype>(
        self,
        slots: &mut WCNodeSlots,
    ) -> Result<LocalCap<D>, RetypeError> {
        if D::SizeBits::U8 > self.cap_data.size_bits {
            return Err(RetypeError::NotBigEnough);
        }

        let slot = slots.alloc(1)?;
        unsafe {
            seL4_Untyped_Retype(
                self.cptr,            // _service
                D::sel4_type_id(),    // type
                0,                    // size_bits
                slots.cptr,           // root
                0,                    // index
                0,                    // depth
                slot.cap_data.offset, // offset
                1,                    // num_objects
            )
        }
        .as_result()
        .map_err(|err| RetypeError::SeL4RetypeError(SeL4Error::UntypedRetype(err)))?;

        Ok(Cap::wrap_cptr(slot.cap_data.offset))
    }
}

#[derive(Debug, PartialEq)]
pub enum WUntypedSplitError {
    TooSmallToBeSplit,
    MemoryRegionWouldExceedAddressableSpace,
    UntypedRetypeError(KernelError),
}

impl LocalCap<WUntyped<memory_kind::Device>> {
    pub fn paddr(&self) -> usize {
        self.cap_data.kind.paddr
    }
}

impl<BitSize: Unsigned> PhantomCap for Untyped<BitSize, memory_kind::General> {
    fn phantom_instance() -> Self {
        Untyped::<BitSize, memory_kind::General> {
            kind: memory_kind::General,
            _bit_size: PhantomData::<BitSize>,
        }
    }
}

impl<BitSize: Unsigned, Kind: MemoryKind> Movable for Untyped<BitSize, Kind> {}

impl<Kind: MemoryKind> Movable for WUntyped<Kind> {}

impl<BitSize: Unsigned, Kind: MemoryKind> Delible for Untyped<BitSize, Kind> {}

pub trait MemoryKind:
    private::SealedMemoryKind + Copy + Clone + core::fmt::Debug + Sized + PartialEq
{
    fn weaken(&self) -> WeakMemoryKind;
    fn halve(&self, size_bytes: usize) -> Option<(Self, Self)>;
    fn quarter(&self, size_bytes: usize) -> Option<(Self, Self, Self, Self)>;
    fn offset_by(&self, bytes: usize) -> Option<Self>;
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum WeakMemoryKind {
    General,
    Device { paddr: usize },
}
pub mod memory_kind {
    use super::MemoryKind;

    #[derive(Clone, Copy, Debug, PartialEq)]
    pub struct General;
    impl MemoryKind for General {
        fn weaken(&self) -> super::WeakMemoryKind {
            super::WeakMemoryKind::General
        }

        fn halve(&self, _size_bytes: usize) -> Option<(Self, Self)> {
            Some((General, General))
        }

        fn quarter(&self, _size_bytes: usize) -> Option<(Self, Self, Self, Self)> {
            Some((General, General, General, General))
        }

        fn offset_by(&self, _bytes: usize) -> Option<Self> {
            Some(General)
        }
    }

    #[derive(Clone, Copy, Debug, PartialEq)]
    pub struct Device {
        /// Physical address at the start of the memory this untyped represents
        pub(crate) paddr: usize,
    }
    impl MemoryKind for Device {
        fn weaken(&self) -> super::WeakMemoryKind {
            super::WeakMemoryKind::Device { paddr: self.paddr }
        }
        fn halve(&self, size_bytes: usize) -> Option<(Self, Self)> {
            if let Some(_) = self.paddr.checked_add(size_bytes) {
                Some((
                    Device { paddr: self.paddr },
                    Device {
                        paddr: self.paddr + size_bytes / 2,
                    },
                ))
            } else {
                None
            }
        }

        fn quarter(&self, size_bytes: usize) -> Option<(Self, Self, Self, Self)> {
            if let Some(_) = self.paddr.checked_add(size_bytes) {
                let subregion_size = size_bytes / 4;
                Some((
                    Device { paddr: self.paddr },
                    Device {
                        paddr: self.paddr + subregion_size,
                    },
                    Device {
                        paddr: self.paddr + subregion_size * 2,
                    },
                    Device {
                        paddr: self.paddr + subregion_size * 3,
                    },
                ))
            } else {
                None
            }
        }

        fn offset_by(&self, bytes: usize) -> Option<Self> {
            if let Some(b) = self.paddr.checked_add(bytes) {
                Some(Device { paddr: b })
            } else {
                None
            }
        }
    }
}

#[derive(Debug)]
pub enum RetypeError {
    CapSizeOverflow,
    BitSizeOverflow,
    KernelRetypeFanOutLimit,
    NotBigEnough,
    SeL4RetypeError(SeL4Error),
    CNodeSlotsError(CNodeSlotsError),
}

impl From<SeL4Error> for RetypeError {
    fn from(e: SeL4Error) -> RetypeError {
        RetypeError::SeL4RetypeError(e)
    }
}

impl From<CNodeSlotsError> for RetypeError {
    fn from(e: CNodeSlotsError) -> RetypeError {
        RetypeError::CNodeSlotsError(e)
    }
}

impl<BitSize: Unsigned, Kind: MemoryKind> LocalCap<Untyped<BitSize, Kind>> {
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
                kind: self.cap_data.kind.clone(),
            },
            _role: PhantomData,
        });

        // Clean up any child/derived capabilities that may have been created.
        unsafe {
            seL4_CNode_Revoke(
                parent_cnode.cptr,   // _service
                self.cptr,           // index
                seL4_WordBits as u8, // depth
            )
        }
        .as_result()
        .map_err(|e| SeL4Error::CNodeRevoke(e))?;
        Ok(r)
    }

    /// weaken erases the type-level state-tracking (size).
    pub fn weaken(self) -> LocalCap<WUntyped<Kind>> {
        Cap {
            cptr: self.cptr,
            cap_data: WUntyped {
                size_bits: BitSize::U8,
                kind: self.cap_data.kind,
            },
            _role: PhantomData,
        }
    }
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

        unsafe {
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
        }
        .as_result()
        .map_err(|e| SeL4Error::UntypedRetype(e))?;
        let (kind_a, kind_b) = self.cap_data.kind.halve(2usize.pow(BitSize::U32))
            // TODO - consider piping this out as a proper error
            .expect("Somehow the backing memory was discovered to cover an invalid memory range when paired with the expected size");
        Ok((
            Cap {
                cptr: dest_offset,
                cap_data: Untyped {
                    kind: kind_a,
                    _bit_size: PhantomData,
                },
                _role: PhantomData,
            },
            Cap {
                cptr: dest_offset + 1,
                cap_data: Untyped {
                    kind: kind_b,
                    _bit_size: PhantomData,
                },
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
        unsafe {
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
        }
        .as_result()
        .map_err(|e| SeL4Error::UntypedRetype(e))?;

        let (kind_a, kind_b, kind_c, kind_d) = self.cap_data.kind.quarter(2usize.pow(BitSize::U32))
            // TODO - consider piping this out as a proper error
            .expect("Somehow the backing memory was discovered to cover an invalid memory range when paired with the expected size");
        Ok((
            Cap {
                cptr: dest_offset,
                cap_data: Untyped {
                    kind: kind_a,
                    _bit_size: PhantomData,
                },
                _role: PhantomData,
            },
            Cap {
                cptr: dest_offset + 1,
                cap_data: Untyped {
                    kind: kind_b,
                    _bit_size: PhantomData,
                },
                _role: PhantomData,
            },
            Cap {
                cptr: dest_offset + 2,
                cap_data: Untyped {
                    kind: kind_c,
                    _bit_size: PhantomData,
                },
                _role: PhantomData,
            },
            Cap {
                cptr: dest_offset + 3,
                cap_data: Untyped {
                    kind: kind_d,
                    _bit_size: PhantomData,
                },
                _role: PhantomData,
            },
        ))
    }

    pub fn retype_memory<CRole: CNodeRole>(
        self,
        dest_slots: CNodeSlots<GranuleSlotCount<BitSize>, CRole>,
    ) -> Result<
        CapRange<Granule<granule_state::Unmapped>, role::Local, GranuleSlotCount<BitSize>>,
        SeL4Error,
    >
    where
        BitSize: IsGreaterOrEqual<PageBits>,
        GranuleSlotCount<BitSize>: IsLessOrEqual<KernelRetypeFanOutLimit, Output = True>,
    {
        let (dest_cptr, dest_offset, _) = dest_slots.elim();
        let gran_info = arch::determine_best_granule_fit(BitSize::U8);
        unsafe {
            seL4_Untyped_Retype(
                self.cptr,         // _service
                gran_info.type_id, //type
                0,                 // size_bits
                dest_cptr,         // root
                0,                 // index
                0,                 // depth
                dest_offset,       // offset
                gran_info.count,   // num_objects
            )
            .as_result()
            .map_err(|e| SeL4Error::UntypedRetype(e))?;
        }

        Ok(CapRange::new(
            dest_offset,
            Page {
                state: granule_state::Unmapped,
            },
        ))
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
    untyped: LocalCap<Untyped<Sum<ChildRadix, CNodeSlotBits>, memory_kind::General>>,
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

    ChildRadix: Add<CNodeSlotBits>,
    Sum<ChildRadix, CNodeSlotBits>: Unsigned,
    Sum<ChildRadix, CNodeSlotBits>: IsGreaterOrEqual<Sum<ChildRadix, CNodeSlotBits>>,
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

        unsafe {
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
        }
        .as_result()
        .map_err(|e| SeL4Error::UntypedRetype(e))?;

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
        unsafe {
            Self::retype_multi_internal(
                self.cptr,
                Count::USIZE,
                TargetCapType::sel4_type_id(),
                dest_cptr,
                dest_offset,
            )?;
        }
        Ok(CapRange::new_phantom(dest_offset))
    }

    unsafe fn retype_multi_internal(
        self_cptr: usize,
        count: usize,
        type_id: usize,
        dest_cptr: usize,
        dest_offset: usize,
    ) -> Result<(), SeL4Error> {
        seL4_Untyped_Retype(
            self_cptr,   // _service
            type_id,     // type
            0,           // size_bits
            dest_cptr,   // root
            0,           // index
            0,           // depth
            dest_offset, // offset
            count,       // num_objects
        )
        .as_result()
        .map_err(|e| SeL4Error::UntypedRetype(e))
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

        ChildRadix: Add<CNodeSlotBits>,
        Sum<ChildRadix, CNodeSlotBits>: Unsigned,
        BitSize: IsGreaterOrEqual<Sum<ChildRadix, CNodeSlotBits>>,
    {
        let (scratch_slot, local_slots) = local_slots.alloc::<U1>();
        let (dest_slot, _) = local_slots.alloc::<U1>();

        let (scratch_cptr, scratch_offset, _) = scratch_slot.elim();
        let (dest_cptr, dest_offset, _) = dest_slot.elim();

        unsafe {
            // Retype to fill the scratch slot with a fresh CNode
            seL4_Untyped_Retype(
                self.cptr,                               // _service
                api_object_seL4_CapTableObject as usize, // type
                ChildRadix::to_usize(),                  // size_bits
                scratch_cptr,                            // root
                0,                                       // index
                0,                                       // depth
                scratch_offset,                          // offset
                1,                                       // num_objects
            )
            .as_result()
            .map_err(|e| SeL4Error::UntypedRetype(e))?;

            // In order to set the guard (for the sake of our C-pointer simplification scheme),
            // mutate the CNode in the scratch slot, which copies the CNode into a second slot
            let guard_data = seL4_CNode_CapData_new(
                0,                                                      // guard
                (seL4_WordBits - ChildRadix::to_usize() as usize) as _, // guard size in bits
            )
            .words[0];

            seL4_CNode_Mutate(
                dest_cptr,           // _service: seL4_CNode,
                dest_offset,         // dest_index: seL4_Word,
                seL4_WordBits as u8, // dest_depth: seL4_Uint8,
                scratch_cptr,        // src_root: seL4_CNode,
                scratch_offset,      // src_index: seL4_Word,
                seL4_WordBits as u8, // src_depth: seL4_Uint8,
                guard_data as usize, // badge or guard: seL4_Word,
            )
            .as_result()
            .map_err(|e| SeL4Error::CNodeMutate(e))?;

            // TODO - If we wanted to make more efficient use of our available slots at the cost
            // of complexity, we could swap the two created CNodes, then delete the one with
            // the incorrect guard (the one originally occupying the scratch slot).
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

impl LocalCap<Untyped<PageBits, memory_kind::Device>> {
    /// The only thing memory_kind::Device memory can be used to make
    /// is a frame.
    pub fn retype_device_frame(
        self,
        dest_slot: LocalCNodeSlot,
    ) -> Result<LocalCap<Granule<granule_state::Unmapped>>, SeL4Error> {
        // Note that we special case introduce a device page creation function
        // because the most likely alternative would be complicating the DirectRetype
        // trait to allow some sort of associated-type matching between the allowable
        // source Untyped memory kinds and the output cap types.

        let (dest_cptr, dest_offset, _) = dest_slot.elim();

        unsafe {
            seL4_Untyped_Retype(
                self.cptr,            // _service
                Page::sel4_type_id(), // type
                0,                    // size_bits
                dest_cptr,            // root
                0,                    // index
                0,                    // depth
                dest_offset,          // offset
                1,                    // num_objects
            )
        }
        .as_result()
        .map_err(|e| SeL4Error::UntypedRetype(e))?;

        Ok(Cap {
            cptr: dest_offset,
            cap_data: Page {
                state: granule_state::Unmapped,
                // TODO - reinstate kind piping
                //memory_kind: self.cap_data.kind,
            },
            _role: PhantomData,
        })
    }
}

impl<BitSize: Unsigned> LocalCap<Untyped<BitSize, memory_kind::Device>> {
    /// Physical address at the start of the memory this untyped represents
    pub(crate) fn paddr(&self) -> usize {
        self.cap_data.kind.paddr
    }
}

mod private {
    pub trait SealedMemoryKind {}
    impl SealedMemoryKind for super::memory_kind::Device {}
    impl SealedMemoryKind for super::memory_kind::General {}
}
