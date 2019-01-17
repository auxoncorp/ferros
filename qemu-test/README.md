This is an external test driver to exercise top-level process functionality by
running a test image under qemu and inspecting its output.

Run the tests with `cargo test`.

You may be asked to press your yubikey during the test, as building the test
project is part of the test, and it will have to fetch dependencies the first
time. If you don't, you'll get strange build failures.
