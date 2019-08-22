(
    export SEL4_PLATFORM=virt

    # message-format=json shows where the tests got built to
    TEST_BINS=$(cargo xtest \
                      -p hello-printer \
                      --target=aarch64-unknown-linux-gnu \
                      --no-run \
                      -vv \
                      --message-format=json \
                    | jq -R 'fromjson? | select(type == "object") | select(.profile.test == true) | .filenames[]'
             )

    echo "~~~~~~~~~~~ built hello-printer tests. Test binaries are:"
    echo $TEST_BINS

    # doctests can't currently be cross-compiled. See https://github.com/rust-lang/cargo/pull/6892

)
