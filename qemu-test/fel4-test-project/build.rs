use std::env;

fn main() {
    let test_case =
        env::var("TEST_CASE").expect("The name of the test case to build must be set in TEST_CASE");

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
