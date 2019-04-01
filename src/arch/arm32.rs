use crate::pow::Pow;
use typenum::operator_aliases::{Diff, Prod, Sum};
use typenum::*;

pub mod paging {
    use super::*;

    pub type BaseASIDPoolFreeSlots = U1024;

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

    // The root task has a stack size configurable by the fel4.toml
    // in the `[fel4.executable]` table's `root-task-stack-bytes` property.
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

    pub const USIZE_PER_PAGE: usize = PageBytes::USIZE / core::mem::size_of::<usize>();
}

pub mod address_space {
    use super::*;

    // TODO this is a magic numbers we got from inspecting the binary.
    /// 0x00010000
    pub type ProgramStart = Pow<U16>;

    /// 0xe0000000
    pub type KernelReservedStart = Sum<Pow<U31>, Sum<Pow<U30>, Pow<U29>>>;
}
