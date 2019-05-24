#! /usr/bin/env bash

# This script is meant to run any and all tests or verification steps
# present in this repository as a top-level check all. If you add
# tests in a new location remember to add them here!

set -e

echo "============================= ./qemu-test ===================================="
cd qemu-test && cargo test && cd ../

echo "==================== ./ferros-test/test-macro-impl ==========================="
cd ferros-test/test-macro-impl && cargo test && cd ../../

echo "==================== ./ferros-test/examples/minimal =========================="
cd ferros-test/examples/minimal && \
    # We can't simulate this because `selfe` waits for a SIGINT to
    # free the caller from QEMU.
    selfe build --platform sabre --sel4_arch aarch32 && \
    cd ../../../

echo "====================== ./ferros-test/examples/mock ==========================="
cd ferros-test/examples/mock && cargo build && cd ../../../
