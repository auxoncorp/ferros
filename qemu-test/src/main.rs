extern crate regex;
extern crate rexpect;

fn main() {}

use lazy_static::lazy_static;
use regex::Regex;
use rexpect::session::spawn_command;
use rexpect::process::signal::Signal;
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

fn run_qemu_test(name: &str, pass_line: Regex, fail_line: Regex) {
    println!("running 'TEST_CASE={} cargo fel4 build", name);


    let result = Command::new("cargo")
        .arg("fel4")
        .arg("build")
        .current_dir("fel4-test-project")
        .env("TEST_CASE", name)
        .output()
        .expect("Couldn't build test project");

    assert!(result.status.success());

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
            );
        }
    }

    sequential_test! {
        fn test_process_runs() {
            run_qemu_test(
                "process_runs",
                Regex::new(".*The value inside the process is 42.*").unwrap(),
                Regex::new(".*Root task should never return from main.*").unwrap(),
            );
        }
    }

    sequential_test! {
        fn memory_read_protection() {
            run_qemu_test(
                "memory_read_protection",
                Regex::new(".*vm fault on data.*").unwrap(),
                Regex::new(".*Root task should never return from main.*").unwrap(),
            );
        }
    }

    sequential_test! {
        fn memory_write_protection() {
            run_qemu_test(
                "memory_write_protection",
                Regex::new(".*vm fault on data.*").unwrap(),
                Regex::new(".*Root task should never return from main.*").unwrap(),
            );
        }
    }
}
