# Rust Target Specifications for feL4

cargo-fel4 ships with support for the following targets:

* [armv7-sel4-fel4](armv7-sel4-fel4.json)
* [aarch64-sel4-fel4](aarch64-sel4-fel4.json)
* [x86_64-sel4-fel4](x86_64-sel4-fel4.json)

## Using custom target specifications

cargo-fel4 uses standard Rust target specification files, a JSON description of the target provided
to the Rust compiler. More information about target specifications can be found on the [rust-cross](https://github.com/japaric/rust-cross#target-specification-files) page.

You can use existing target specifications for your target or modify the ones provided here.

Rust will search for target specifications in the directory specified
by the environment variable `RUST_TARGET_PATH`.

cargo-fel4 will construct `RUST_TARGET_PATH` from the `target-specs-path` property
in a project's `fel4.toml` manifest, relative to the project's root directory.

See the [rust-cross](https://github.com/japaric/rust-cross) page for more information
on cross compiling Rust programs.
