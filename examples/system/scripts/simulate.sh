#!/usr/bin/env bash

set -e

./scripts/fetch-toolchain.sh

export PATH="$(pwd)"/target/toolchains/gcc-linaro-7.4.1-2019.02-i686_arm-linux-gnueabihf/bin:$PATH

./scripts/mkflash.sh

selfe simulate \
    --platform sabre \
    --sel4_arch aarch32 \
    --serial-override='-serial telnet:0.0.0.0:8888,server,nowait -serial mon:stdio' \
    -- \
    -smp 4 \
    -drive if=mtd,file=target/flash/flash.bin,format=raw,id=spi,index=0,bus=0 \
    -nic tap,mac=00:AD:BE:EF:CA:FE,ifname=qemu-net,script=no,downscript=no

exit 0
