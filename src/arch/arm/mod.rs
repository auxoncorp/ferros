#![cfg(target_arch = "arm")]

use typenum::*;

pub mod cap;

pub type WordSize = U32;
pub type MinUntypedSize = U4;
// MaxUntypedSize is half the address space and/or word size.
pub type MaxUntypedSize = op!(WordSize - U1);

pub type ASIDControlBits = U2;
pub type ASIDPoolBits = U10;
pub type ASIDPoolCount = op!(U1 << ASIDControlBits);
pub type ASIDPoolSize = op!(U1 << ASIDPoolBits);

pub type PageDirectoryBits = U12;
pub type PageTableBits = U8;
pub type PageBits = U12;
pub type PageBytes = op!(U1 << U12);

pub type BasePageDirFreeSlots = op!((U1 << PageDirectoryBits) - (U1 << U9));
pub type BasePageTableFreeSlots = op!(U1 << PageTableBits);

// TODO remove these when elf stuff lands.
// this is a magic numbers we got from inspecting the binary.
/// 0x00010000
pub type ProgramStart = op!(U1 << U16);
pub type CodePageTableBits = U6;
pub type CodePageTableCount = op!(U1 << CodePageTableBits); // 64 page tables == 64 mb
pub type CodePageCount = op!(CodePageTableCount * BasePageTableFreeSlots); // 2^14
pub type TotalCodeSizeBits = U26;
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
pub type RootTaskReservedPageDirSlots = op!(CodePageTableCount + RootTaskStackPageTableCount);
pub type RootTaskPageDirFreeSlots = op!(BasePageDirFreeSlots - RootTaskReservedPageDirSlots);

/// 0xe0000000
pub type KernelReservedStart = op!((U1 << U31) + (U1 << U30) + (U1 << U29));

pub const WORDS_PER_PAGE: usize = PageBytes::USIZE / core::mem::size_of::<usize>();
