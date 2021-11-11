#!/usr/bin/env bash
# 2MB flash

set -e

RUSTDOCFLAGS="--enable-index-page -Zunstable-options" cargo +nightly doc --target armv7-unknown-linux-gnueabihf --no-deps

xdg-open target/armv7-unknown-linux-gnueabihf/doc/index.html

exit 0
