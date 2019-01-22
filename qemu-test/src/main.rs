extern crate regex;
extern crate rexpect;

fn main() {}

use lazy_static::lazy_static;
use regex::Regex;
use rexpect::process::signal::Signal;
use rexpect::session::spawn_command;
use std::process::Command;
use std::sync::Mutex;

lazy_static! {
    static ref SEQUENTIAL_TEST_MUTEX: Mutex<()> = Mutex::new(());
}
macro_rules! sequential_test {
    (fn $name:ident() $body:block) => {
        #[test]
        fn $name() {
            let _guard = $crate::SEQUENTIAL_TEST_MUTEX.lock();
            {
                $body
            }
        }
    };
}

fn run_qemu_test(
    name: &str,
    pass_line: Regex,
    fail_line: Regex,
    supplemental_feature_flags: Option<Vec<(&'static str, &'static str)>>,
) {
    let rust_identifier_regex: Regex =
        Regex::new("(^[a-zA-Z][a-zA-Z0-9_]*$)|(^_[a-zA-Z0-9_]+$)").unwrap();
    let is_rust_id = |s| rust_identifier_regex.is_match(s);
    if !is_rust_id(name) {
        panic!(
            "Invalid test case name {}. Test case name must be a valid rust identifier",
            name
        );
    }

    let mut build_command = Command::new("cargo");
    (&mut build_command)
        .arg("fel4")
        .arg("build")
        .current_dir("fel4-test-project")
        .env("TEST_CASE", name);
    let escaped_flags_summary = {
        if let Some(flags) = supplemental_feature_flags {
            let merged_pairs: Vec<_> = flags
                .iter()
                .map(|(k, v)| {
                    if !is_rust_id(k) || !is_rust_id(v) {
                        panic!("Invalid extra test feature flag passed: ({}, {}). Extra flags must be valid rust identifiers", k, v)
                    }
                    format!("{}=\"{}\"", k, v)
                })
                .collect();
            (&mut build_command).env("TEST_EXTRA_FLAG_PAIRS", merged_pairs.join(","));
            let escaped_pairs: Vec<_> = flags
                .iter()
                .map(|(k, v)| format!("{}=\\\"{}\\\"", k, v))
                .collect();
            format!("TEST_EXTRA_FLAG_PAIRS={}", escaped_pairs.join(","))
        } else {
            "".to_string()
        }
    };
    println!(
        "running 'TEST_CASE={} {} cargo fel4 build",
        name, escaped_flags_summary
    );
    let build_result = build_command.output().expect("Couldn't build test project");

    assert!(build_result.status.success());

    println!("running 'cargo fel4 simulate");

    let mut sim_command = Command::new("cargo");
    sim_command
        .arg("fel4")
        .arg("simulate")
        .current_dir("fel4-test-project");

    let mut sim = spawn_command(sim_command, Some(10000)).expect("Couldn't start simulate command");

    loop {
        let line = sim
            .read_line()
            .expect("couldn't read line from simulate process");

        if pass_line.is_match(&line) {
            sim.process.kill(Signal::SIGKILL).unwrap();
            break;
        }

        if fail_line.is_match(&line) {
            sim.process.kill(Signal::SIGKILL).unwrap();
            panic!("Output line matched failure pattern: {}", line);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    sequential_test! {
        fn test_root_task_runs() {
            run_qemu_test(
                "root_task_runs",
                Regex::new(".*hello from the root task.*").unwrap(),
                Regex::new(".*Root task should never return from main.*").unwrap(),
                Some(vec![("single_process", "true"),("min_params", "true")]),
            );
        }
    }

    sequential_test! {
        fn test_process_runs() {
            run_qemu_test(
                "process_runs",
                Regex::new(".*The value inside the process is 42.*").unwrap(),
                Regex::new(".*Root task should never return from main.*").unwrap(),
                Some(vec![("single_process", "true"),("min_params", "true")]),
            );
        }
    }

    sequential_test! {
        fn memory_read_protection() {
            run_qemu_test(
                "memory_read_protection",
                Regex::new(".*vm fault on data.*").unwrap(),
                Regex::new(".*Root task should never return from main.*").unwrap(),
                Some(vec![("single_process", "true"),("min_params", "true")]),
            );
        }
    }

    sequential_test! {
        fn memory_write_protection() {
            run_qemu_test(
                "memory_write_protection",
                Regex::new(".*vm fault on data.*").unwrap(),
                Regex::new(".*Root task should never return from main.*").unwrap(),
                Some(vec![("single_process", "true"),("min_params", "true")]),
            );
        }
    }

    sequential_test! {
        fn child_process_cap_management() {
            run_qemu_test(
                "child_process_cap_management",
                Regex::new(".*Split, retyped, and deleted caps in a child process.*").unwrap(),
                Regex::new(".*Root task should never return from main.*").unwrap(),
                Some(vec![("single_process", "true")]),
            );
        }
    }

    sequential_test! {
        fn over_register_size_params() {
            run_qemu_test(
                "over_register_size_params",

                Regex::new(".*The child process saw a first value of bbbbbbbb, a mid value of aaaaaaaa, and a last value of cccccccc.*").unwrap(),
                Regex::new(".*Root task should never return from main.*").unwrap(),
                Some(vec![("single_process", "true")]),
            );
        }
    }

    sequential_test! {
        fn call_and_response_loop() {
            run_qemu_test(
                "call_and_response_loop",

                Regex::new(".*Call and response addition finished.*").unwrap(),
                Regex::new(".*Root task should never return from main.*").unwrap(),
                Some(vec![("dual_process", "true")]),
            );
        }
    }

    sequential_test! {
        fn fault_pair() {
            run_qemu_test(
                "fault_pair",

                Regex::new(".*Caught a fault: CapFault\\(CapFault \\{ sender: 0, in_receive_phase: false, cap_address: 314159 \\}\\).*").unwrap(),
                Regex::new(".*Root task should never return from main.*").unwrap(),
                Some(vec![("dual_process", "true")]),
            );
        }
    }
}
