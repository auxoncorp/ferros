use core::cmp;
use core::marker::PhantomData;
use core::ops::Sub;

use typenum::*;

use super::{KernelRetypeFanOutLimit, NumPages, VSpaceError};
use crate::arch::{self, PageBits, PageBytes};
use crate::cap::{
    memory_kind, page_state, role, CNode, CNodeRole, CNodeSlots, Cap, CapRange, InternalASID,
    LocalCNodeSlots, LocalCap, MemoryKind, Page, PageState, RetypeError, Untyped, WCNodeSlots,
    WUntyped, WeakCapRange, WeakMemoryKind,
};
use crate::error::SeL4Error;

use crate::pow::{Pow, _Pow};
use crate::userland::CapRights;

pub trait SharedStatus: private::SealedSharedStatus {}

pub mod shared_status {
    use super::SharedStatus;

    pub struct Shared;
    impl SharedStatus for Shared {}

    pub struct Exclusive;
    impl SharedStatus for Exclusive {}
}

mod private {
    use super::shared_status::{Exclusive, Shared};
    pub trait SealedSharedStatus {}
    impl SealedSharedStatus for Shared {}
    impl SealedSharedStatus for Exclusive {}
}
/// A `1 << SizeBits` bytes region of unmapped memory. It can be
/// shared or owned exclusively. The ramifications of its shared
/// status are described more completely in the `mapped_shared_region`
/// function description.
#[allow(type_alias_bounds)]
pub type UnmappedMemoryRegion<SizeBits, ShStatus, CapRole: CNodeRole = role::Local> =
    MemoryRegion<page_state::Unmapped, SizeBits, ShStatus, CapRole>;
/// A memory region which is mapped into an address space, meaning it
/// has a virtual address and an associated asid in which that virtual
/// address is valid.
#[allow(type_alias_bounds)]
pub type MappedMemoryRegion<SizeBits, ShStatus, CapRole: CNodeRole = role::Local> =
    MemoryRegion<page_state::Mapped, SizeBits, ShStatus, CapRole>;
#[allow(type_alias_bounds)]
pub type WeakUnmappedMemoryRegion<ShStatus, CapRole: CNodeRole = role::Local> =
    WeakMemoryRegion<page_state::Unmapped, ShStatus, CapRole>;
#[allow(type_alias_bounds)]
pub type WeakMappedMemoryRegion<ShStatus, CapRole: CNodeRole = role::Local> =
    WeakMemoryRegion<page_state::Mapped, ShStatus, CapRole>;

/// A `1 << SizeBits` bytes region of memory. It can be
/// shared or owned exclusively. The ramifications of its shared
/// status are described more completely in the `mapped_shared_region`
/// function description.
pub struct MemoryRegion<
    State: PageState,
    SizeBits: Unsigned,
    SS: SharedStatus,
    CapRole: CNodeRole = role::Local,
> where
    // Forces regions to be page-aligned.
    SizeBits: IsGreaterOrEqual<PageBits>,
    SizeBits: Sub<PageBits>,
    <SizeBits as Sub<PageBits>>::Output: Unsigned,
    <SizeBits as Sub<PageBits>>::Output: _Pow,
    Pow<<SizeBits as Sub<PageBits>>::Output>: Unsigned,
{
    pub(super) caps: CapRange<Page<State>, CapRole, NumPages<SizeBits>>,
    pub(super) kind: WeakMemoryKind,
    _size_bits: PhantomData<SizeBits>,
    _shared_status: PhantomData<SS>,
}

impl<State: PageState, SizeBits: Unsigned, SS: SharedStatus, CapRole: CNodeRole>
    MemoryRegion<State, SizeBits, SS, CapRole>
where
    SizeBits: IsGreaterOrEqual<PageBits>,
    SizeBits: Sub<PageBits>,
    <SizeBits as Sub<PageBits>>::Output: Unsigned,
    <SizeBits as Sub<PageBits>>::Output: _Pow,
    Pow<<SizeBits as Sub<PageBits>>::Output>: Unsigned,
{
    pub const SIZE_BYTES: usize = 1 << SizeBits::USIZE;

    /// The number of bits needed to address this region
    pub fn size_bits(&self) -> u8 {
        SizeBits::U8
    }

    /// The size of this region in bytes.
    pub fn size_bytes(&self) -> usize {
        Self::SIZE_BYTES
    }

    pub(super) fn from_caps(
        caps: CapRange<Page<State>, CapRole, NumPages<SizeBits>>,
        kind: WeakMemoryKind,
    ) -> MemoryRegion<State, SizeBits, SS, CapRole> {
        MemoryRegion {
            caps,
            kind,
            _size_bits: PhantomData,
            _shared_status: PhantomData,
        }
    }

    pub(super) fn unchecked_new(
        local_page_caps_offset_cptr: usize,
        state: State,
        kind: WeakMemoryKind,
    ) -> Self {
        MemoryRegion {
            caps: CapRange::new(local_page_caps_offset_cptr, Page { state }),
            kind,
            _size_bits: PhantomData,
            _shared_status: PhantomData,
        }
    }
    pub fn weaken(self) -> WeakMemoryRegion<State, SS, CapRole> {
        WeakMemoryRegion::try_from_caps(self.caps.weaken(), self.kind, SizeBits::U8)
            .expect("Cap page slots to memory region size invariant maintained by type signature")
    }

    /// N.B. until MemoryKind tracking is added to Page, this is a lossy
    /// conversion that will assume the Region was for General memory
    pub(crate) fn to_page(self) -> LocalCap<Page<State>>
    where
        SizeBits: IsEqual<PageBits, Output = True>,
    {
        Cap {
            cptr: self.caps.start_cptr,
            cap_data: self.caps.start_cap_data,
            _role: PhantomData,
        }
    }

    pub fn paddr(&self) -> Result<usize, SeL4Error> {
        let page = Cap {
            cptr: self.caps.start_cptr,
            cap_data: self.caps.start_cap_data.clone(),
            _role: PhantomData,
        };
        page.paddr()
    }

    /// In the Ok case, returns a shared, unmapped copy of the memory
    /// region (backed by fresh page-caps) along with this self-same
    /// memory region, marked as shared.
    pub fn share<CNodeSlotCount: Unsigned, DestRole: CNodeRole>(
        self,
        slots: CNodeSlots<CNodeSlotCount, DestRole>,
        cnode: &LocalCap<CNode<CapRole>>,
        rights: CapRights,
    ) -> Result<
        (
            MemoryRegion<page_state::Unmapped, SizeBits, shared_status::Shared, DestRole>,
            MemoryRegion<State, SizeBits, shared_status::Shared, CapRole>,
        ),
        VSpaceError,
    >
    where
        CNodeSlotCount: IsEqual<NumPages<SizeBits>, Output = True>,
    {
        let pages_offset = self.caps.start_cptr;
        let original_mapped_state = self.caps.start_cap_data.state;
        let slots_offset = slots.cap_data.offset;
        for (slot, page) in slots.iter().zip(self.caps.into_iter()) {
            let _ = page.copy(cnode, slot, rights)?;
        }

        Ok((
            MemoryRegion::unchecked_new(slots_offset, page_state::Unmapped, self.kind),
            MemoryRegion::from_caps(
                CapRange::new(
                    pages_offset,
                    Page {
                        state: original_mapped_state,
                    },
                ),
                self.kind,
            ),
        ))
    }
}

impl LocalCap<Page<page_state::Unmapped>> {
    /// N.B. until MemoryKind tracking is added to Page, this is a lossy
    /// conversion that will assume the Page was for General memory
    pub(crate) fn to_region(
        self,
    ) -> MemoryRegion<page_state::Unmapped, PageBits, shared_status::Exclusive> {
        MemoryRegion::unchecked_new(self.cptr, self.cap_data.state, WeakMemoryKind::General)
    }
}

impl<SizeBits: Unsigned> UnmappedMemoryRegion<SizeBits, shared_status::Exclusive>
where
    SizeBits: IsGreaterOrEqual<PageBits>,
    SizeBits: Sub<PageBits>,
    <SizeBits as Sub<PageBits>>::Output: Unsigned,
    <SizeBits as Sub<PageBits>>::Output: _Pow,
    Pow<<SizeBits as Sub<PageBits>>::Output>: Unsigned,
{
    /// Retype the necessary number of granules into memory
    /// capabilities and return the unmapped region.
    pub fn new(
        ut: LocalCap<Untyped<SizeBits>>,
        slots: LocalCNodeSlots<NumPages<SizeBits>>,
    ) -> Result<Self, crate::error::SeL4Error>
    where
        Pow<<SizeBits as Sub<PageBits>>::Output>:
            IsLessOrEqual<KernelRetypeFanOutLimit, Output = True>,
    {
        let kind = ut.cap_data.kind;
        let page_caps = ut.retype_pages(slots)?;
        Ok(UnmappedMemoryRegion::from_caps(page_caps, kind.weaken()))
    }

    pub fn new_device<Role: CNodeRole>(
        ut: LocalCap<Untyped<SizeBits, memory_kind::Device>>,
        slots: CNodeSlots<NumPages<SizeBits>, Role>,
    ) -> Result<Self, crate::error::SeL4Error>
    where
        Pow<<SizeBits as Sub<PageBits>>::Output>:
            IsLessOrEqual<KernelRetypeFanOutLimit, Output = True>,
    {
        let kind = ut.cap_data.kind;
        let page_caps = ut.retype_pages(slots)?;
        Ok(UnmappedMemoryRegion::from_caps(page_caps, kind.weaken()))
    }

    /// A shared region of memory can be duplicated. When it is
    /// mapped, it's _borrowed_ rather than consumed allowing for its
    /// remapping into other address spaces.
    pub fn to_shared(self) -> UnmappedMemoryRegion<SizeBits, shared_status::Shared> {
        UnmappedMemoryRegion::from_caps(self.caps, self.kind)
    }
}

impl<SizeBits: Unsigned, SS: SharedStatus> MappedMemoryRegion<SizeBits, SS>
where
    SizeBits: IsGreaterOrEqual<PageBits>,
    SizeBits: Sub<PageBits>,
    <SizeBits as Sub<PageBits>>::Output: Unsigned,
    <SizeBits as Sub<PageBits>>::Output: _Pow,
    Pow<<SizeBits as Sub<PageBits>>::Output>: Unsigned,
{
    pub fn vaddr(&self) -> usize {
        self.caps.start_cap_data.state.vaddr
    }

    pub(crate) fn asid(&self) -> InternalASID {
        self.caps.start_cap_data.state.asid
    }

    pub fn rights(&self) -> CapRights {
        self.caps.start_cap_data.state.rights
    }

    pub fn as_slice(&self) -> &[u8] {
        unsafe { core::slice::from_raw_parts(self.vaddr() as *const u8, self.size_bytes()) }
    }

    pub fn as_mut_slice(&mut self) -> &mut [u8] {
        unsafe { core::slice::from_raw_parts_mut(self.vaddr() as *mut u8, self.size_bytes()) }
    }

    pub fn flush(&self) -> Result<(), SeL4Error> {
        self.caps.for_each::<SeL4Error, _>(|cap| {
            unsafe {
                arch::flush_page(cap.cptr)?;
            }
            Ok(())
        })?;

        Ok(())
    }

    pub fn flush_range(&self, vaddr: usize, size: usize) -> Result<(), SeL4Error> {
        let bottom = vaddr & !0xFFF;
        let top = vaddr + cmp::max(PageBytes::USIZE, size);
        let range = bottom..top;
        self.caps.for_each::<SeL4Error, _>(|cap| {
            if range.contains(&cap.vaddr()) {
                unsafe {
                    arch::flush_page(cap.cptr)?;
                }
            }
            Ok(())
        })?;

        Ok(())
    }

    #[cfg(feature = "test_support")]
    /// Super dangerous copy-aliasing
    pub(crate) unsafe fn dangerous_internal_alias(&mut self) -> Self {
        MappedMemoryRegion::unchecked_new(
            self.caps.start_cptr,
            page_state::Mapped {
                vaddr: self.vaddr(),
                asid: self.asid(),
                rights: self.rights(),
            },
            self.kind,
        )
    }

    /// Halve a region into two regions.
    pub fn split(
        self,
    ) -> Result<
        (
            MappedMemoryRegion<op!(SizeBits - U1), SS>,
            MappedMemoryRegion<op!(SizeBits - U1), SS>,
        ),
        VSpaceError,
    >
    where
        SizeBits: Sub<U1>,
        <SizeBits as Sub<U1>>::Output: Unsigned,
        <SizeBits as Sub<U1>>::Output: IsGreaterOrEqual<U12, Output = True>,
        <SizeBits as Sub<U1>>::Output: Sub<PageBits>,
        <<SizeBits as Sub<U1>>::Output as Sub<PageBits>>::Output: Unsigned,
        <<SizeBits as Sub<U1>>::Output as Sub<PageBits>>::Output: _Pow,
        Pow<<<SizeBits as Sub<U1>>::Output as Sub<PageBits>>::Output>: Unsigned,
    {
        let new_region_vaddr = if let Some(vaddr) = 2_usize
            .checked_pow(SizeBits::U32 - 1)
            .and_then(|v| v.checked_add(self.vaddr()))
        {
            vaddr
        } else {
            return Err(VSpaceError::ExceededAddressableSpace);
        };

        let new_offset = self.caps.start_cptr + (self.caps.len() / 2);

        Ok((
            MappedMemoryRegion {
                caps: CapRange::new(
                    self.caps.start_cptr,
                    Page {
                        state: page_state::Mapped {
                            vaddr: self.vaddr(),
                            asid: self.asid(),
                            rights: self.rights(),
                        },
                    },
                ),
                kind: self.kind,
                _size_bits: PhantomData,
                _shared_status: PhantomData,
            },
            MappedMemoryRegion {
                caps: CapRange::new(
                    new_offset,
                    Page {
                        state: page_state::Mapped {
                            vaddr: new_region_vaddr,
                            asid: self.asid(),
                            rights: self.rights(),
                        },
                    },
                ),
                kind: self.kind,
                _size_bits: PhantomData,
                _shared_status: PhantomData,
            },
        ))
    }

    /// Splits a range into a specific size and a SizeBits-1 region.
    ///
    /// NB: This function drops on the floor the leftovers between
    /// TargetSize and SizeBits-1 It's only meant to be used to set up
    /// regions for supporting ferros-test.
    ///
    /// Something like:
    /// ```not_rust
    /// SizeBits = 20, TargetSize = 16
    /// [                 20                   ]
    /// [        19        |         19        ]
    /// [        19        | 16 |   dropped    ]
    /// ```
    #[cfg(feature = "test_support")]
    pub fn split_into<TargetSize: Unsigned>(
        self,
    ) -> Result<
        (
            MappedMemoryRegion<TargetSize, SS>,
            MappedMemoryRegion<op!(SizeBits - U1), SS>,
        ),
        VSpaceError,
    >
    where
        TargetSize: IsGreaterOrEqual<PageBits>,
        TargetSize: Sub<PageBits>,
        <TargetSize as Sub<PageBits>>::Output: Unsigned,
        <TargetSize as Sub<PageBits>>::Output: _Pow,
        Pow<<TargetSize as Sub<PageBits>>::Output>: Unsigned,

        SizeBits: Sub<U1>,
        <SizeBits as Sub<U1>>::Output: Unsigned,
        <SizeBits as Sub<U1>>::Output: IsGreaterOrEqual<U12, Output = True>,
        <SizeBits as Sub<U1>>::Output: Sub<PageBits>,
        <<SizeBits as Sub<U1>>::Output as Sub<PageBits>>::Output: Unsigned,
        <<SizeBits as Sub<U1>>::Output as Sub<PageBits>>::Output: _Pow,
        Pow<<<SizeBits as Sub<U1>>::Output as Sub<PageBits>>::Output>: Unsigned,
    {
        let (a, b) = self.split()?;

        Ok((
            MappedMemoryRegion {
                caps: CapRange::new(
                    a.caps.start_cptr,
                    Page {
                        state: page_state::Mapped {
                            vaddr: a.vaddr(),
                            asid: a.asid(),
                            rights: a.rights(),
                        },
                    },
                ),
                kind: a.kind,
                _size_bits: PhantomData,
                _shared_status: PhantomData,
            },
            b,
        ))
    }
}

pub struct WeakMemoryRegion<State: PageState, SS: SharedStatus, CapRole: CNodeRole = role::Local> {
    pub(super) caps: WeakCapRange<Page<State>, CapRole>,
    pub(super) kind: WeakMemoryKind,
    size_bits: u8,
    _shared_status: PhantomData<SS>,
}

impl WeakMemoryRegion<page_state::Unmapped, shared_status::Exclusive> {
    pub fn new<MemKind: MemoryKind>(
        untyped: LocalCap<WUntyped<MemKind>>,
        slots: &mut WCNodeSlots,
    ) -> Result<Self, RetypeError> {
        let kind = untyped.cap_data.kind.weaken();
        let size_bits = untyped.size_bits();
        let caps = untyped.retype_pages(slots)?;
        Ok(WeakMemoryRegion {
            caps,
            kind,
            size_bits,
            _shared_status: PhantomData,
        })
    }
}
impl<State: PageState, SS: SharedStatus> WeakMemoryRegion<State, SS, role::Local> {
    pub(super) fn unchecked_new(
        local_page_caps_offset_cptr: usize,
        state: State,
        kind: WeakMemoryKind,
        size_bits: u8,
    ) -> Self {
        let num_pages = num_pages(size_bits)
            .expect("Calling functions maintain the invariant that the size_bits is over the size of a page");
        WeakMemoryRegion {
            caps: WeakCapRange::new(local_page_caps_offset_cptr, Page { state }, num_pages),
            kind,
            size_bits,
            _shared_status: PhantomData,
        }
    }
}
impl<State: PageState, SS: SharedStatus, CapRole: CNodeRole> WeakMemoryRegion<State, SS, CapRole> {
    /// The number of bits needed to address this region
    pub fn size_bits(&self) -> u8 {
        self.size_bits
    }

    /// The size of this region in bytes.
    pub fn size_bytes(&self) -> usize {
        2usize.pow(u32::from(self.size_bits))
    }
    pub(super) fn try_from_caps(
        caps: WeakCapRange<Page<State>, CapRole>,
        kind: WeakMemoryKind,
        size_bits: u8,
    ) -> Result<WeakMemoryRegion<State, SS, CapRole>, InvalidSizeBits> {
        if num_pages(size_bits)? != caps.len() {
            return Err(InvalidSizeBits::SizeBitsMismatchPageCapCount);
        }
        Ok(WeakMemoryRegion {
            caps,
            kind,
            size_bits,
            _shared_status: PhantomData,
        })
    }

    pub(super) fn as_strong<SizeBits: Unsigned>(
        self,
    ) -> Result<MemoryRegion<State, SizeBits, SS, CapRole>, VSpaceError>
    where
        // Forces regions to be page-aligned.
        SizeBits: IsGreaterOrEqual<PageBits>,
        SizeBits: Sub<PageBits>,
        <SizeBits as Sub<PageBits>>::Output: Unsigned,
        <SizeBits as Sub<PageBits>>::Output: _Pow,
        Pow<<SizeBits as Sub<PageBits>>::Output>: Unsigned,
    {
        if self.size_bits != SizeBits::U8 {
            return Err(VSpaceError::InvalidRegionSize);
        }
        Ok(MemoryRegion::from_caps(
            CapRange::new(self.caps.start_cptr, self.caps.start_cap_data),
            self.kind,
        ))
    }

    pub fn to_shared(self) -> WeakMemoryRegion<State, shared_status::Shared, CapRole> {
        WeakMemoryRegion {
            caps: self.caps,
            kind: self.kind,
            size_bits: self.size_bits,
            _shared_status: PhantomData,
        }
    }
}

impl<SS: SharedStatus, CapRole: CNodeRole> WeakMappedMemoryRegion<SS, CapRole> {
    pub fn vaddr(&self) -> usize {
        self.caps.start_cap_data.state.vaddr
    }

    pub(crate) fn asid(&self) -> InternalASID {
        self.caps.start_cap_data.state.asid
    }

    pub fn as_slice(&self) -> &[u8] {
        unsafe { core::slice::from_raw_parts(self.vaddr() as *const u8, self.size_bytes()) }
    }

    pub fn as_mut_slice(&mut self) -> &mut [u8] {
        unsafe { core::slice::from_raw_parts_mut(self.vaddr() as *mut u8, self.size_bytes()) }
    }
}

#[derive(Debug, PartialEq)]
pub(super) enum InvalidSizeBits {
    TooSmallToRepresentAPage,
    SizeBitsMismatchPageCapCount,
    SizeBitsTooBig,
}

pub(super) fn num_pages(size_bits: u8) -> Result<usize, InvalidSizeBits> {
    if size_bits < PageBits::U8 {
        return Err(InvalidSizeBits::TooSmallToRepresentAPage);
    }
    2usize
        .checked_pow(u32::from(size_bits) - PageBits::U32)
        .ok_or(InvalidSizeBits::SizeBitsTooBig)
}
