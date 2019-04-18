use std::env;

fn main() {
    println!("cargo:rerun-if-env-changed=TEST_CASE");
    println!("cargo:rerun-if-env-changed=TEST_EXTRA_FLAG_PAIRS");

    let (test_case, extra_flag_pairs) = match env::var("TEST_CASE") {
        Ok(val) => (val, env::var("TEST_EXTRA_FLAG_PAIRS")),
        Err(_) => (
            "root_task_runs".to_string(),
            Ok(r#"single_process="true",min_params="true""#.to_string()),
        ),
    };

    println!("cargo:rustc-cfg=test_case=\"{}\"", test_case);

    if let Ok(flags) = extra_flag_pairs {
        // Assume pair is already in the format of key=\"value\"
        for pair in flags.split(",") {
            if !pair.is_empty() {
                println!("cargo:rustc-cfg={}", pair);
            }
        }
    }
}
