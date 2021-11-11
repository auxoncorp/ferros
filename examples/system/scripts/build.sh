#!/usr/bin/env bash

set -e

./scripts/fetch-toolchain.sh

export PATH="$(pwd)"/target/toolchains/gcc-linaro-7.4.1-2019.02-i686_arm-linux-gnueabihf/bin:$PATH

selfe build --platform sabre --sel4_arch aarch32

exit 0
