use core::fmt;
use core::marker::PhantomData;
use core::mem;
use core::ops::{Add, Sub};
use core::slice;
use crate::userland::SeL4Error;
use typenum::{Diff, IsLess, NonZero, Sum, True, Unsigned, U0};

// TODO - replacing internal cap with CapRange or similar
pub type DmaCacheOpCapToken = usize;

#[derive(Debug)]
pub enum MemoryRegionError {
    InsufficientMemory,
    NotSupported,
    OutOfBounds,
    SeL4Error(SeL4Error),
}

impl From<SeL4Error> for MemoryRegionError {
    fn from(s: SeL4Error) -> Self {
        MemoryRegionError::SeL4Error(s)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Address {
    Virtual(usize),
    Physical(usize),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MemoryRegion<VAddr: Unsigned = U0, PAddr: Unsigned = U0, SizeBytes: Unsigned = U0> {
    _vaddr_marker: PhantomData<VAddr>,
    _paddr_marker: PhantomData<PAddr>,
    _size_marker: PhantomData<SizeBytes>,
    // TODO - cap to page dir for now
    pub(crate) cache_op_token: Option<DmaCacheOpCapToken>,
}

impl MemoryRegion {
    // TODO - there could be a world where paddr 0x0 is valid
    //
    /// Creates a new `MemoryRegion` without capabilities to perform
    /// DMA cache operations (device memory for example).
    pub fn new<
        VAddr: Unsigned + NonZero,
        PAddr: Unsigned + NonZero,
        SizeBytes: Unsigned + NonZero,
    >() -> MemoryRegion<VAddr, PAddr, SizeBytes> {
        MemoryRegion {
            _paddr_marker: PhantomData,
            _vaddr_marker: PhantomData,
            _size_marker: PhantomData,
            cache_op_token: None,
        }
    }

    /// Creates a new `MemoryRegion` with capabilities to perform
    /// DMA cache operations.
    pub fn new_with_token<
        VAddr: Unsigned + NonZero,
        PAddr: Unsigned + NonZero,
        SizeBytes: Unsigned + NonZero,
    >(
        token: DmaCacheOpCapToken,
    ) -> MemoryRegion<VAddr, PAddr, SizeBytes> {
        MemoryRegion {
            _paddr_marker: PhantomData,
            _vaddr_marker: PhantomData,
            _size_marker: PhantomData,
            cache_op_token: Some(token),
        }
    }
}

impl<VAddr: Unsigned, PAddr: Unsigned, SizeBytes: Unsigned> MemoryRegion<VAddr, PAddr, SizeBytes> {
    pub fn vaddr(&self) -> usize {
        VAddr::USIZE
    }

    pub fn paddr(&self) -> usize {
        PAddr::USIZE
    }

    pub fn size(&self) -> usize {
        SizeBytes::USIZE
    }

    /// Returns true if the `MemoryRegion` contains the address.
    pub fn contains(&self, address: Address) -> bool {
        let (start, addr) = match address {
            Address::Virtual(addr) => (self.vaddr(), addr),
            Address::Physical(addr) => (self.paddr(), addr),
        };

        if (addr >= start) && (addr < (start + self.size())) {
            true
        } else {
            false
        }
    }

    /// Returns true if the `MemoryRegion` contains the address range.
    pub fn contains_range(&self, address: Address, size: usize) -> bool {
        if self.contains(address) == true {
            let (start, addr) = match address {
                Address::Virtual(addr) => (self.vaddr(), addr),
                Address::Physical(addr) => (self.paddr(), addr),
            };

            if (addr + size) <= (start + self.size()) {
                true
            } else {
                false
            }
        } else {
            false
        }
    }

    /// Shrinks the capacity of the `MemoryRegion` with a lower bound.
    pub fn shrink_to<Size>(self) -> MemoryRegion<VAddr, PAddr, Size>
    where
        Size: Unsigned + NonZero + IsLess<SizeBytes, Output = True>,
    {
        MemoryRegion {
            _paddr_marker: PhantomData,
            _vaddr_marker: PhantomData,
            _size_marker: PhantomData,
            cache_op_token: self.cache_op_token,
        }
    }

    /// Splits the `MemoryRegion` into two at the given *byte* offset.
    ///
    /// Returns a newly allocated tuple (head region, tail region).
    /// Head region contains `[0, ByteOffset)`,
    /// and the tail region contains `[ByteOffset, end)`.
    pub fn split_off<ByteOffset>(
        self,
    ) -> (
        MemoryRegion<VAddr, PAddr, ByteOffset>,
        MemoryRegion<Sum<VAddr, ByteOffset>, Sum<PAddr, ByteOffset>, Diff<SizeBytes, ByteOffset>>,
    )
    where
        VAddr: Add<ByteOffset>,
        PAddr: Add<ByteOffset>,
        SizeBytes: Sub<ByteOffset>,
        ByteOffset: Unsigned + NonZero + IsLess<SizeBytes, Output = True>,
        Sum<VAddr, ByteOffset>: Unsigned,
        Sum<PAddr, ByteOffset>: Unsigned,
        Diff<SizeBytes, ByteOffset>: Unsigned,
    {
        (
            MemoryRegion {
                _paddr_marker: PhantomData,
                _vaddr_marker: PhantomData,
                _size_marker: PhantomData,
                cache_op_token: self.cache_op_token,
            },
            MemoryRegion {
                _paddr_marker: PhantomData,
                _vaddr_marker: PhantomData,
                _size_marker: PhantomData,
                cache_op_token: self.cache_op_token,
            },
        )
    }

    /// Returns a raw pointer to the `MemoryRegion`.
    pub fn as_ptr<T>(&self) -> Result<*const T, MemoryRegionError> {
        if mem::size_of::<T>() > SizeBytes::USIZE {
            Err(MemoryRegionError::InsufficientMemory)
        } else {
            Ok(self.vaddr() as *const T)
        }
    }

    /// Returns a mutable raw pointer to the `MemoryRegion`.
    pub fn as_mut_ptr<T>(&mut self) -> Result<*mut T, MemoryRegionError> {
        if mem::size_of::<T>() > SizeBytes::USIZE {
            Err(MemoryRegionError::InsufficientMemory)
        } else {
            Ok(self.vaddr() as *mut T)
        }
    }

    /// Extracts a slice containing the number of elements.
    pub unsafe fn as_slice<T>(&self, len: usize) -> Result<&[T], MemoryRegionError> {
        if (len * mem::size_of::<T>()) > SizeBytes::USIZE {
            Err(MemoryRegionError::InsufficientMemory)
        } else {
            Ok(slice::from_raw_parts(self.as_ptr()?, len))
        }
    }

    /// Extracts a mutable slice containing the number of elements.
    pub unsafe fn as_mut_slice<T>(&mut self, len: usize) -> Result<&mut [T], MemoryRegionError> {
        if (len * mem::size_of::<T>()) > SizeBytes::USIZE {
            Err(MemoryRegionError::InsufficientMemory)
        } else {
            Ok(slice::from_raw_parts_mut(self.as_mut_ptr()?, len))
        }
    }
}

impl<VAddr: Unsigned, PAddr: Unsigned, SizeBytes: Unsigned> fmt::Display
    for MemoryRegion<VAddr, PAddr, SizeBytes>
{
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(
            f,
            "MemoryRegion {{ vaddr {:#010X} paddr {:#010X} size: {:#010X} }}",
            self.vaddr(),
            self.paddr(),
            self.size(),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use typenum::{Add1, Sum, U1, U10, U128, U16, U2, U256, U3, U31, U32, U4, U64};

    #[test]
    fn mem_reg_new() {
        let mem = MemoryRegion::new::<U1, U2, U3>();

        assert_eq!(mem.vaddr(), U1::USIZE);
        assert_eq!(mem.paddr(), U2::USIZE);
        assert_eq!(mem.size(), U3::USIZE);
    }

    #[test]
    fn address_in_bounds() {
        type Vaddr = U256;
        type Paddr = U128;
        type Size = U32;
        let mem = MemoryRegion::new::<Vaddr, Paddr, Size>();

        {
            type Offset = U0;
            type InBoundsVaddr = Sum<Vaddr, Offset>;
            assert_eq!(mem.contains(Address::Virtual(Offset::USIZE)), false);
            assert_eq!(mem.contains(Address::Virtual(InBoundsVaddr::USIZE)), true);
            assert_eq!(
                mem.contains_range(Address::Virtual(InBoundsVaddr::USIZE), Size::USIZE),
                true
            );
            type OutBoundsSize = Add1<Size>;
            assert_eq!(
                mem.contains_range(Address::Virtual(InBoundsVaddr::USIZE), OutBoundsSize::USIZE),
                false
            );
            type InBoundsPaddr = Sum<Paddr, Offset>;
            assert_eq!(mem.contains(Address::Physical(Offset::USIZE)), false);
            assert_eq!(mem.contains(Address::Physical(InBoundsPaddr::USIZE)), true);
            assert_eq!(
                mem.contains_range(Address::Physical(InBoundsPaddr::USIZE), Size::USIZE),
                true
            );
            assert_eq!(
                mem.contains_range(
                    Address::Physical(InBoundsPaddr::USIZE),
                    OutBoundsSize::USIZE
                ),
                false
            );
        }

        {
            type Offset = U16;
            type InBoundsVaddr = Sum<Vaddr, Offset>;
            assert_eq!(mem.contains(Address::Virtual(Offset::USIZE)), false);
            assert_eq!(mem.contains(Address::Virtual(InBoundsVaddr::USIZE)), true);
            type InBoundsPaddr = Sum<Paddr, Offset>;
            assert_eq!(mem.contains(Address::Physical(Offset::USIZE)), false);
            assert_eq!(mem.contains(Address::Physical(InBoundsPaddr::USIZE)), true);
        }

        {
            type Offset = U31;
            type InBoundsVaddr = Sum<Vaddr, Offset>;
            assert_eq!(mem.contains(Address::Virtual(Offset::USIZE)), false);
            assert_eq!(mem.contains(Address::Virtual(InBoundsVaddr::USIZE)), true);
            type InBoundsPaddr = Sum<Paddr, Offset>;
            assert_eq!(mem.contains(Address::Physical(Offset::USIZE)), false);
            assert_eq!(mem.contains(Address::Physical(InBoundsPaddr::USIZE)), true);
        }
    }

    #[test]
    fn shrinking() {
        type Vaddr = U256;
        type Paddr = U128;
        type OrigSize = U32;
        let mem = MemoryRegion::new::<Vaddr, Paddr, OrigSize>();
        assert_eq!(mem.size(), OrigSize::USIZE);

        type SmallerSize = U10;
        let mem = mem.shrink_to::<SmallerSize>();
        assert_eq!(mem.size(), SmallerSize::USIZE);

        type SmallestSize = U1;
        let mem = mem.shrink_to::<SmallestSize>();
        assert_eq!(mem.size(), SmallestSize::USIZE);
    }

    #[test]
    fn splits() {
        type Vaddr = U128;
        type Paddr = U64;
        type OrigSize = U256;
        let mem = MemoryRegion::new::<Vaddr, Paddr, OrigSize>();
        assert_eq!(mem.size(), OrigSize::USIZE);

        type SplitAt = U128;
        let (head_mem, tail_mem) = mem.split_off::<SplitAt>();

        assert_eq!(head_mem.vaddr(), Vaddr::USIZE);
        assert_eq!(head_mem.paddr(), Paddr::USIZE);
        assert_eq!(head_mem.size(), SplitAt::USIZE);

        type TailVaddr = Sum<Vaddr, SplitAt>;
        type TailPaddr = Sum<Paddr, SplitAt>;
        type TailSize = Diff<OrigSize, SplitAt>;
        assert_eq!(tail_mem.vaddr(), TailVaddr::USIZE);
        assert_eq!(tail_mem.paddr(), TailPaddr::USIZE);
        assert_eq!(tail_mem.size(), TailSize::USIZE);
    }

    #[test]
    fn raw_pointers() {
        type Vaddr = U128;
        type Paddr = U64;
        type Size = U4;

        let mem = MemoryRegion::new::<Vaddr, Paddr, Size>();
        assert_eq!(mem.size(), Size::USIZE);
        assert_eq!(mem.as_ptr::<u8>().unwrap() as usize, Vaddr::USIZE);
    }

    #[test]
    fn slices() {
        type Vaddr = U128;
        type Paddr = U64;
        type Size = U64;

        let mem = MemoryRegion::new::<Vaddr, Paddr, Size>();
        assert_eq!(mem.size(), Size::USIZE);
        let slice = unsafe { mem.as_slice::<u8>(8).unwrap() };
        assert_eq!(&slice[0] as *const _ as usize, Vaddr::USIZE);
    }
}
