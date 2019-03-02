# ferros

## Overview

A Rust library to add extra assurances to seL4 development.

`ferros` provides smart type-safe wrappers around seL4 features
with an emphasis on compile-time resource tracking.

`ferros` builds on top of the `libsel4-sys` library from the `feL4` ecosystem.

## Build

Install `cargo-fel4` then run `cargo fel4 build` from the root project directory.

Integration test execution is as simple as `cd qemu-test && cargo test` and requires the installation of `qemu-system-arm`.

## Usage

Add `ferros` to your `feL4` project in the `Cargo.toml`

```
[dependencies]
ferros = { git = "ssh://github.com/auxoncorp/ferros"}
```

## Quick Start

The following literate-code walkthrough assumes execution in a feL4 project,
and introduces some key concepts and features of `ferros`.

```
use sel4_sys;
use ferros::micro_alloc::Allocator;
use ferros::userland::{root_cnode, BootInfo};

// The raw boot info is provided by the default feL4 entry point;
let raw_boot_info: &'static sel4_sys::seL4_BootInfo = unimplemented!();

// Create the top-level CNode wrapper with type-level-tracked remaining slot capacity
let root_cnode: LocalCap<CNode<_>> = root_cnode(&raw_boot_info);

// Utility for finding and claiming `Untyped` instances supplied by the boot info.
let mut allocator = micro_alloc::Allocator::bootstrap(&raw_boot_info)?;
let initial_untyped: LocalCap<Untyped<U20>> = allocator
    .get_untyped::<U20>() // The size of the Untyped instance, as bits
    .expect("Couldn't find an untyped instance of the desired size");

// Once we have an initial Untyped instance, memory distribution from it
// can be tracked with compile-time checks. We split up large
// Untyped instances to the right size for transformation into useful
// kernel objects.
let (ut18, ut18b, _ut18c, _ut18d, root_cnode) = ut20.quarter(root_cnode)?;
let (ut16, _, _, _, root_cnode) = ut18.quarter(root_cnode)?;
// Note the CNode that will contain the (here, Untyped) capabilities
// is passed in to the function, and then returned
// with diminished type-tracked slot capacity.
let (ut14, _, _, _, root_cnode) = ut18.quarter(root_cnode)?;

// Create a page table seL4 kernel object and return a capability pointer to it.
// Here we use a variable bidning type annotation and Rust's type system can figure out
// if the Untyped instance used has enough capacity to represent this particular
// kernel object.
let (root_page_table, root_cnode): (LocalCap<UnmappedPageTable>, _) =
    ut18b.retype_local(root_cnode)?;

// Create a resource-tracking wrapper around the raw boot info to assist in
// virtual memory related operations.
let (boot_info, root_cnode) = BootInfo::wrap(raw_boot_info, asid_pool_ut, root_cnode);
let (root_page_table, boot_info) = boot_info.map_page_table(root_page_table)?;
```
