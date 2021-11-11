#!/usr/bin/env bash

set -e

dir=$(pwd)
toolchains_dir="${dir}/target/toolchains"
mkdir -p $toolchains_dir

armv7_toolchain="gcc-linaro-7.4.1-2019.02-i686_arm-linux-gnueabihf"
armv7_toolchain_url="https://releases.linaro.org/components/toolchain/binaries/7.4-2019.02/arm-linux-gnueabihf/${armv7_toolchain}.tar.xz"
armv7_toolchain_dir="${toolchains_dir}/${armv7_toolchain}"

if [ ! -d "${armv7_toolchain_dir}" ]; then
    (
        cd target/toolchains
        curl -LO $armv7_toolchain_url
        tar xf "${armv7_toolchain}.tar.xz"
    )
fi

exit 0
