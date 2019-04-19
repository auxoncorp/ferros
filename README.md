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

The following code walkthrough assumes execution in a selfe project,
and introduces some aspects of `ferros`.

```
use selfe_sys;
use ferros::micro_alloc::Allocator;
use ferros::userland::{root_cnode, BootInfo};

// The raw boot info is provided by the default feL4 entry point;
let raw_boot_info: &'static selfe_sys::seL4_BootInfo = unimplemented!();

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
// Here we use a variable binding type annotation and Rust's type system can figure out
// if the Untyped instance used has enough capacity to represent this particular
// kernel object.
let (root_page_table, root_cnode): (LocalCap<UnmappedPageTable>, _) =
    ut18b.retype_local(root_cnode)?;

// Create a resource-tracking wrapper around the raw boot info to assist in
// virtual memory related operations.
let (boot_info, root_cnode) = BootInfo::wrap(raw_boot_info, asid_pool_ut, root_cnode);
let (root_page_table, boot_info) = boot_info.map_page_table(root_page_table)?;
```

## Features

### Context-Aware Automatic Capability Management

Capabilities are the mechanism by which seL4 applications manage their
access to useful kernel objects, like notifications, endpoints, and pages.

Capabilities are stored in specialized collections of capability-holding-capacity,
called CNodes. In basic seL4 development, knowledge of a complex addressing scheme
(CPointers) is required to generate and manipulate capabilities. In C seL4 development there
are few guard-rails against misinterpreting what type of kernel object is referenced
by a CPointer, let alone whether that CPointer is even valid in the current execution
context.

`ferros` solves these problems by tracking capabilities with a smart pointer type, `Cap`.

`Cap` pointers are parameterized at the type level by the kernel object kind they point at,
as well as by whether the pointer is valid in the local execution context (or in the context
of a child process); e.g. `Cap<Endpoint, Local>` , `Cap<Notification, Child>`

The `ferros` APIs


### Compile Time Resource Management

seL4 offers several resources worth tracking -- available free slots in a `CNode`, raw memory
in `Untyped` instances, the portions of virtual memory space yet unclaimed, and so
forth. `ferros` tracks these resources at compile time.

`CNode`s have a self-explanatory `FreeSlots` type parameter, `Untyped`s have a `BitSize` type parameter
that shows how many bits of memory they could store, and `VSpace`'s `PageDirFreeSlots`
and `PageTableFreeSlots` params collaborate to show which portions of virtual address space
have yet to be claimed or mapped.

Whenever a function needs to take some subset of resources from these objects,
the function consumes the object as a whole and returns an instance of that object
with the necessary type parameters decremented to track the resources expended. If a resource
container's contents aren't sufficient to a given task at hand, the developer will experience
a compile time failure (as opposed to a runtime one).

Following initialization the `ferros` framework renders accidental resource exhaustion in production deployment impossible
by making correct usage mandatory during design and development.

### Isolation and Communication

Atop the basic building blocks of capability creation and storage, `ferros` provides higher-level
primitives for creating fully isolated subprocesses (see `vspace.rs`) as well as several
options for well-typed synchronous and asynchronous communication between processes
(see `multi_consumer.rs`, `ipc.rs`, and `shared_memory_ipc.rs`).
