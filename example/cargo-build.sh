# build all packages in the right order, so binary packaging works as expected.
set -e

if [ -z ${SEL4_CONFIG_PATH+x} ]; then
    echo "SEL4_CONFIG_PATH is unset; set it, or build with 'selfe'";
    exit 1;
fi

if [ -z ${SEL4_PLATFORM+x} ]; then
    echo "SEL4_PLATFORM is unset; set it, or build with 'selfe'";
    exit 1;
fi

# reversed topological sort of the dep graph
for c in $(tsort crate-binary-deps | tac); do
    echo "---------------- building ${c} ----------------"
    cargo xbuild -p $c $@;
done

