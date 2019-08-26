set -e

(
    # export SEL4_PLATFORM=virt

    rm -f target/aarch64-unknown-linux-gnu/debug/*-*
    ./cargo-build.sh --target=aarch64-unknown-linux-gnu

    echo "building tests"
    # message-format=json shows where the tests got built to
    # export FERROS_TEST_FILTER=pass
    cargo xtest \
          --target=aarch64-unknown-linux-gnu \
          --no-run
                    #   --message-format=json \
                    # | jq -R 'fromjson? | select(type == "object") | select(.profile.test == true) | .filenames[]'

    # echo "~~~~~~~~~~~ built hello-printer tests. Test binaries are:"
    # echo $TEST_BINS

    # ls target/aarch64-unknown-linux-gnu/debug/*-*

    # doctests can't currently be cross-compiled. See https://github.com/rust-lang/cargo/pull/6892

    # build the testrunner root task
    # mkdir -p .testrunner

    cargo xbuild -p fancy-test-runner --target=aarch64-unknown-linux-gnu
)
