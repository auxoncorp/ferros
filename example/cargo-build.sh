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

if [ -z ${TEST+x} ]; then
    echo "----------- building apps and libs -----------"
    cargo xbuild --all --exclude "root-task" $@

    echo "----------- building root task ---------------"
    cargo xbuild -p "root-task" $@;

else
    rm target/armv7-unknown-linux-gnueabi/debug/* || true
    rm target/aarch64-unknown-linux-gnu/debug/* || true

    echo "--------------- building tests ---------------"
    cargo xtest --no-run $@

    echo "------------- building test runner -----------"
    cargo xbuild -p "fancy-test-runner" $@

    echo "----- replacing root task with test runner----"
    rm target/armv7-unknown-linux-gnueabi/debug/root-task || true
    cp target/armv7-unknown-linux-gnueabi/debug/fancy-test-runner target/armv7-unknown-linux-gnueabi/debug/root-task || true

    rm target/aarch64-unknown-linux-gnu/debug/root-task || true
    cp target/aarch64-unknown-linux-gnu/debug/fancy-test-runner target/aarch64-unknown-linux-gnu/debug/root-task || true
fi
