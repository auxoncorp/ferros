use core::{fmt, mem};

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Error {
    InvalidSize,
}

/// An uncached contiguous physical memory region
#[derive(Debug, Copy, Clone)]
pub struct UncachedMemoryRegion {
    /// Virtual address
    vaddr: usize,
    /// Physical address
    paddr: usize,
    /// Size in bytes
    size: usize,
}

impl UncachedMemoryRegion {
    /// # Safety
    ///
    /// Make sure not to mess this up
    pub unsafe fn new(vaddr: usize, paddr: usize, size: usize) -> Self {
        UncachedMemoryRegion { vaddr, paddr, size }
    }

    /// Returns the virtual address of the UncachedMemoryRegion.
    pub fn vaddr(&self) -> usize {
        self.vaddr
    }

    /// Returns the physical address of the UncachedMemoryRegion.
    pub fn paddr(&self) -> usize {
        self.paddr
    }

    /// Returns the number of *bytes* in the UncachedMemoryRegion.
    pub fn size(&self) -> usize {
        self.size
    }

    /// Splits the UncachedMemoryRegion into two at the given *byte* index.
    ///
    /// Returns a newly allocated `Self`.
    /// `self` contains bytes `[0, at)` and
    /// the returned `Self` contains bytes `[at, len)`.
    pub fn split_off(&mut self, at: usize) -> Result<Self, Error> {
        if at >= self.size {
            Err(Error::InvalidSize)
        } else {
            let mut tail_region = UncachedMemoryRegion {
                vaddr: self.vaddr,
                paddr: self.paddr,
                size: self.size,
            };

            // New region consumes tail
            tail_region.vaddr += at;
            tail_region.paddr += at;
            tail_region.size -= at;

            // Our region consumes the head, ending at the offset
            self.size = at;

            Ok(tail_region)
        }
    }

    /// Splits the UncachedMemoryRegion into two at the given *byte* index.
    ///          
    /// Returns a newly allocated `Self`.
    /// `self` contains bytes `[at, len)` and
    /// the returned `Self` contains bytes `[0, at)`.
    pub fn split(&mut self, at: usize) -> Result<Self, Error> {
        if at >= self.size {
            Err(Error::InvalidSize)
        } else {
            let mut head_region = UncachedMemoryRegion {
                vaddr: self.vaddr,
                paddr: self.paddr,
                size: self.size,
            };

            // New region consumes the head
            head_region.size = at;

            // Our region consumes the tail
            self.vaddr += at;
            self.paddr += at;
            self.size -= at;

            Ok(head_region)
        }
    }

    /// Shrinks the capacity of the UncachedMemoryRegion with a lower bound.
    pub fn shrink_to(&mut self, size: usize) -> Result<(), Error> {
        if size > self.size {
            Err(Error::InvalidSize)
        } else {
            self.size = size;
            Ok(())
        }
    }

    /// Extracts a slice containing the number of words.
    pub fn as_slice<T>(&self, count: usize) -> &[T] {
        debug_assert!(self.size >= (mem::size_of::<T>() * count));
        unsafe { core::slice::from_raw_parts(self.as_ptr(), count) }
    }

    /// Extracts a mutable slice containing the number of words.
    pub fn as_mut_slice<T>(&mut self, count: usize) -> &mut [T] {
        debug_assert!(self.size >= (mem::size_of::<T>() * count));
        unsafe { core::slice::from_raw_parts_mut(self.as_mut_ptr(), count) }
    }

    /// Returns a raw pointer to the UncachedMemoryRegion.
    pub fn as_ptr<T>(&self) -> *const T {
        debug_assert!(self.size >= mem::size_of::<T>());
        self.vaddr as *const T
    }

    /// Returns a mutable raw pointer to the UncachedMemoryRegion.
    pub fn as_mut_ptr<T>(&self) -> *mut T {
        debug_assert!(self.size >= mem::size_of::<T>());
        self.vaddr as *mut T
    }
}

impl fmt::Display for UncachedMemoryRegion {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(
            f,
            "UncachedMemoryRegion {{ vaddr {:#010X}, paddr {:#010X}, size: {:#010X} }}",
            self.vaddr(),
            self.paddr(),
            self.size(),
        )
    }
}
