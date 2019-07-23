#! /usr/bin/env bash

# This script is meant to run any and all tests or verification steps
# present in this repository as a top-level check all. If you add
# tests in a new location remember to add them here!

set -e

dir=$(pwd)

echo "====================== download toolchains =================================="
toolchains_dir="${dir}/target/toolchains"
mkdir -p $toolchains_dir

armv7_toolchain="gcc-linaro-7.4.1-2019.02-i686_arm-linux-gnueabihf"
armv7_toolchain_url="https://releases.linaro.org/components/toolchain/binaries/latest-7/arm-linux-gnueabihf/${armv7_toolchain}.tar.xz"
armv7_toolchain_dir="${toolchains_dir}/${armv7_toolchain}"

if [ ! -d "${armv7_toolchain_dir}" ]; then
    (
        cd target/toolchains
        curl -LO $armv7_toolchain_url
        tar xf "${armv7_toolchain}.tar.xz"
    )
else
    echo "Using existing armv7 toolchain at ${armv7_toolchain_dir}"
fi

armv8_toolchain="gcc-linaro-7.4.1-2019.02-i686_aarch64-linux-gnu"
armv8_toolchain_url="https://releases.linaro.org/components/toolchain/binaries/latest-7/aarch64-linux-gnu/${armv8_toolchain}.tar.xz"
armv8_toolchain_dir="${toolchains_dir}/${armv8_toolchain}"

if [ ! -d "${armv8_toolchain_dir}" ]; then
    (
        cd target/toolchains
        curl -LO $armv8_toolchain_url
        tar xf "${armv8_toolchain}.tar.xz"
    )
else
    echo "Using existing aarch64 toolchain at ${armv8_toolchain_dir}"
fi

echo "========================= build aarch64 (tx1) =================================="
(
    export PATH="${armv8_toolchain_dir}/bin:${PATH}"

    SEL4_PLATFORM=tx1 \
        SEL4_CONFIG_PATH="$dir/sel4.toml" \
        cargo xbuild --target aarch64-unknown-linux-gnu --features "test_support"
)


echo "======================== build aarch32 (sabre) ================================="
(
    export PATH="${armv7_toolchain_dir}/bin:${PATH}"

    SEL4_PLATFORM=sabre \
        SEL4_CONFIG_PATH="$dir/sel4.toml" \
        cargo xbuild --target armv7-unknown-linux-gnueabihf --features "test_support"
)


echo "==================== ./ferros-test/test-macro-impl ==========================="
(
    cd ferros-test/test-macro-impl
    cargo test
)

echo "==================== ./ferros-test/examples/minimal =========================="
(
    export PATH="${armv7_toolchain_dir}/bin:${PATH}"
    cd ferros-test/examples/minimal
    # We can't simulate this because `selfe` waits for a SIGINT to
    # free the caller from QEMU.
    selfe build --platform sabre --sel4_arch aarch32
)

echo "====================== ./ferros-test/examples/mock ==========================="
(
    cd ferros-test/examples/mock
    cargo build
)

echo "====================== ./smart_alloc ==========================="
(
    cd smart_alloc
    cargo test
)

echo "====================== ./cross_queue ==========================="
(
    cd cross_queue
    cargo test
)

echo "============================= ./qemu-test ===================================="
(
    export PATH="${armv7_toolchain_dir}/bin:${armv8_toolchain_dir}/bin:${PATH}"
    cd qemu-test
    cargo test
)
