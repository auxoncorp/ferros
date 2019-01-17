use std::env;

fn main() {
    let test_case =
        env::var("TEST_CASE").expect("The name of the test case to build must be set in TEST_CASE");

    println!("cargo:rustc-cfg=test_case=\"{}\"", test_case);
}
