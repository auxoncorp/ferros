use std::env;

fn main() {
    println!("cargo:rerun-if-env-changed=TEST_CASE");
    println!("cargo:rerun-if-env-changed=TEST_EXTRA_FLAG_PAIRS");

    let test_case =
        env::var("TEST_CASE").expect("The name of the test case to build must be set in TEST_CASE");
    let test_case = env::var("TEST_CASE").unwrap();

    println!("cargo:rustc-cfg=test_case=\"{}\"", test_case);

    if let Ok(flags) = env::var("TEST_EXTRA_FLAG_PAIRS") {
        // Assume pair is already in the format of key=\"value\"
        for pair in flags.split(",") {
            if !pair.is_empty() {
                println!("cargo:rustc-cfg={}", pair);
            }
        }
    }
}
