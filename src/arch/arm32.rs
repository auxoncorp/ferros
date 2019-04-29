use crate::pow::Pow;
use typenum::operator_aliases::{Diff, Prod, Sum};
use typenum::*;

pub type WordBits = U32;
pub type WordBytes = U4;

pub mod kernel {
    use super::*;
    pub type MaxUntypedSize = U31;
    pub type MinUntypedSize = U4;
}

pub mod asid {
    use super::*;

    pub type ControlBits = U2;
    pub type PoolBits = U10;

    pub type PoolCount = op! {U1 << ControlBits};
    pub type PoolSize = op! {U1 << PoolBits };
}

pub mod paging {
    use super::*;

    // Arm32 address structure
    pub type PageDirectoryBits = U12;
    pub type PageTableBits = U8;
    pub type PageBits = U12; // 4kb

    pub type LargePageBits = U16; // 64 KB
    pub type SectionBits = U20; // 1 MB
    pub type SuperSectionBits = U24; // 16 MB

    // PageTableBits + PageBits
    pub type PageTableTotalBits = U20;

    pub type CodePageTableBits = U6;
    pub type CodePageTableCount = Pow<CodePageTableBits>; // 64 page tables == 64 mb
    pub type CodePageCount = Prod<CodePageTableCount, BasePageTableFreeSlots>; // 2^14
    pub type TotalCodeSizeBits = U26;

    // 0xe00000000 and up is reserved to the kernel; this translates to the last
    // 2^9 (512) pagedir entries.
    pub type BasePageDirFreeSlots = Diff<Pow<PageDirectoryBits>, Pow<U9>>;

    pub type BasePageTableFreeSlots = Pow<PageTableBits>;

    // The root task has a stack size configurable by the sel4.toml
    // in the `root-task-stack-bytes` metadata property.
    // This configuration is turned into a generated Rust type named `RootTaskStackPageTableCount`
    // that implements `typenum::Unsigned` in the `build.rs` file.
    include!(concat!(
        env!("OUT_DIR"),
        "/ROOT_TASK_STACK_PAGE_TABLE_COUNT"
    ));
    // The first N page tables are already mapped for the user image in the root
    // task. Add in the stack-reserved page tables (minimum of 1 more)
    pub type RootTaskReservedPageDirSlots = Sum<CodePageTableCount, RootTaskStackPageTableCount>;

    pub type RootTaskPageDirFreeSlots = Diff<BasePageDirFreeSlots, RootTaskReservedPageDirSlots>;

    // Useful for constant comparison to data structure size_of results
    pub type PageBytes = Pow<PageBits>;

    pub const WORDS_PER_PAGE: usize = PageBytes::USIZE / core::mem::size_of::<usize>();
}

pub mod address_space {
    use super::*;

    // TODO this is a magic numbers we got from inspecting the binary.
    /// 0x00010000
    pub type ProgramStart = Pow<U16>;

    /// 0xe0000000
    pub type KernelReservedStart = Sum<Pow<U31>, Sum<Pow<U30>, Pow<U29>>>;
}

pub mod ut_buddy {
    use super::*;
    pub type UTPoolSlotsPerSize = U4;
}
