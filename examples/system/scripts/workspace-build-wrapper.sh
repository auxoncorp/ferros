#!/usr/bin/env bash

set -e

if [ -z ${SEL4_CONFIG_PATH+x} ]; then
    echo "SEL4_CONFIG_PATH is unset; set it, or build with 'selfe'";
    exit 1;
fi

if [ -z ${SEL4_PLATFORM+x} ]; then
    echo "SEL4_PLATFORM is unset; set it, or build with 'selfe'";
    exit 1;
fi

# build all packages in the right order, so binary packaging works as expected.
echo "======================= building iomux ======================"
cargo build -p iomux $@;

echo "======================= building enet ======================"
cargo build -p enet $@;

echo "======================= building tcpip ======================"
cargo build -p tcpip $@;

echo "======================= building persistent-storage ======================"
cargo build -p persistent-storage $@;

echo "======================= building console ======================"
cargo build -p console $@;

echo "======================== building root-task ======================="
cargo build -p root-task $@;

exit 0
