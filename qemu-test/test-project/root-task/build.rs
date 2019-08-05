use std::env;

fn main() {
    println!("cargo:rerun-if-env-changed=TEST_CASE");

    let test_case = match env::var("TEST_CASE") {
        Ok(val) => val,
        Err(_) => "root_task_runs".to_string(),
    };

    println!("cargo:rustc-cfg=test_case=\"{}\"", test_case);
}
