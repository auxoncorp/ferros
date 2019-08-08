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
echo "======================= building elf-process ======================"
cargo xbuild -p elf-process $@;

echo "======================== building root-task ======================="
cargo xbuild -p root-task $@;
