# ferros-test

## Overview

A Rust library providing functions and macros in support of testing code that
must execute in a seL4 context and makes use of seL4-kernel-supplied resources.

## Build

Install `cargo-xbuild` then run `cargo xbuild --target
armv7-unknown-linux-gnueabihf` from the root project directory.

```bash
# build the library
cargo build

# build the examples
cd examples/minimal
SEL4_PLATFORM=sabre cargo xbuild --target armv7-unknown-linux-gnueabihf

cd ../mock
cargo build
cd ..
```

## Usage

Add `ferros-test` as a cargo dependency to a [selfe style application project](https://github.com/auxoncorp/selfe-sys)
that will serve as the test harness project.

```toml
[dependencies]
ferros-test = { git = "ssh://git@github.com/auxoncorp/ferros.git"}
```

### Creating Tests

In the source of that project, you define tests as functions that accept zero or more ferros 
capabilities return a Result, annotated with a `#[ferros_test]` attribute.

```rust
use ferros::*;
use ferros_test::ferros_test;

#[ferros_test]
fn example_test(ut: LocalCap<Untyped<U5>>, slots: LocalCNodeSlots<U4>) -> Result<(), SeL4Error> {
    ut.split(slots).map(|_| ())
}
```

An `Ok(_)` returned indicates test success, while an `Err(_)` result is interpreted as test failure.
The actual type parameters of Result<T, E> are fully ignored.

The test framework will attempt to allocate and pass the seL4 resources requested through the function parameters.
These resources are drawn from a recycled pool. Objects created or derived during the execution of a
test body must not exceed the lifetime of the test.

#### Supported test parameters

Tests may be parameterized by arguments of the following types:

* `LocalCNodeSlots<_>`
* `LocalCap<CNodeSlots<_>>`
* `LocalCap<Untyped<_>>`
* `LocalCap<ASIDPool<_>>`
  * Only a single ASIDPool argument is supported per test, with a maximum of 1024 slots
* `&mut VSpaceScratchSlice`
  * Only a single VSpaceScratchSlice argument is supported per test
* `&UserImage<Local>`
* `&LocalCap<LocalCNode>`

### Running Tests

You execute tests by passing a slice of such-annotated functions to the  `execute_tests` helper function,
supplied with test-resources extracted from a `seL4_BootInfo` object. [sel4-start](https://github.com/auxoncorp/selfe-sys/tree/master/example_application/sel4-start) is one way to get
a handle on a boot info object.

```rust
use ferros::test_support::{execute_tests, Resources};

fn main() {
    let raw_boot_info = unsafe { &*sel4_start::BOOTINFO };
    let (mut resources, reporter) = Resources::with_debug_reporting(raw_boot_info)
        .expect("Test resource setup failed");
    execute_tests(
        reporter,
        resources.as_mut_ref(),
        &[
            &example_test,
            &other_example_test,
            &more_detailed_integration_test
        ]).expect("Test execution failed");
}
```

Once you execute this application with, say, `selfe simulate --platform sabre --sel4_arch aarch32`,
the outcome of executing the tests should be printed.
See the `minimal` application in the examples subdirectory for a full demonstration.

## Tests

The tests for this library itself can be invoked with:

```bash
cargo test
```

## Notes

This approach to test annotation and execution differs from that of the default Rust test framework,
as well as the custom test framework system that is under development. The biggest gaps arise
from the fact that panic-catching infrastructure is not readily built-in to the `#![no_std]`
environment in which seL4 applications currently operate.  At present, `cargo` does not support setting

```toml
[profile.test]
panic = "abort"
```

Such efforts are ignored (most likely because the standard test framework catches panics to detect failures),
leading to linking errors when panic-unwind-supporting code is required as part of the build
process.

Similarly, the custom test framework system at present is not fully integrated into cargo
in a `#![no_std]` compatible way. Aggregate results are not incorporated in the final `cargo test` textual output.
The custom test framework currently defines `impl Termination` as the output type for test harness implementations,
(with the intention of using that to guide the `cargo test` process exit code), but the `Termination` trait currently
is only defined in `std::process`, not `core::anywhere`.

In the future, when these an other modest indignities are fixed, it would be reasonable to adjust this
and related libraries to be more a more direct toolkit for supporting the construction of flexible
test seL4 harnesses. Some likely changes to support this would be:
 
 * Adjust the `#[ferros_test]` macro to export `#[test_case]` as a decorating attribute for the output function
   * This gives us automatic test gathering so the harness need no longer explicitly receive a list of tests to run
 * Move the test harness setup shown in the Usage section above to an independent custom-test-harness framework subcrate.
   * This test harness subcrate will likely also pull in (or depend upon) sel4-start like functionality directly
 * Expand the use of feature-flagging to parameterize harness/framework fit-to-platform-under-test rather
 than leaning on end-user manual selection of desired reporting strategy.
