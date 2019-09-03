extern crate regex;
extern crate rexpect;

fn main() {}

use lazy_static::lazy_static;
use regex::Regex;
use rexpect::process::signal::Signal;
use rexpect::session::spawn_command;
use std::io::{self, Write};
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

#[derive(Debug, Clone, Copy)]
pub enum TestPlatform {
    /// A virtual aarch64 platform similar to the tx1
    VirtTx1Aarch64,
    /// The sabre aarch32
    SabreAarch32,
}

impl TestPlatform {
    fn sel4_arch(&self) -> &'static str {
        match self {
            TestPlatform::VirtTx1Aarch64 => "aarch64",
            TestPlatform::SabreAarch32 => "aarch32",
        }
    }
    fn platform(&self) -> &'static str {
        match self {
            TestPlatform::VirtTx1Aarch64 => "virt",
            TestPlatform::SabreAarch32 => "sabre",
        }
    }
}

fn run_qemu_test<F>(
    test_case: &str,
    pass_line: Regex,
    fail_line: Regex,
    ready_line_and_func: Option<(Regex, F)>,
    serial_override: Option<&str>,
    test_platform: TestPlatform,
) where
    F: Fn(),
{
    let rust_identifier_regex: Regex =
        Regex::new("(^[a-zA-Z][a-zA-Z0-9_]*$)|(^_[a-zA-Z0-9_]+$)").unwrap();
    let is_rust_id = |s| rust_identifier_regex.is_match(s);
    if !is_rust_id(test_case) {
        panic!(
            "Invalid test case test_case {}. Test case name must be a valid rust identifier",
            test_case
        );
    }

    let mut build_command = Command::new("selfe");
    (&mut build_command)
        .arg("build")
        .arg("--sel4_arch")
        .arg(test_platform.sel4_arch())
        .arg("--platform")
        .arg(test_platform.platform())
        .arg("-v")
        .current_dir("test-project")
        .env("TEST_CASE", test_case);

    println!(r#"running: TEST_CASE={} {:?}"#, test_case, build_command);
    let build_result = build_command.output().expect("Couldn't run `selfe build`");
    if !build_result.status.success() {
        io::stdout().write_all(&build_result.stdout).unwrap();
        io::stderr().write_all(&build_result.stderr).unwrap();
    }
    assert!(build_result.status.success());

    let mut sim_command = Command::new("selfe");
    sim_command.arg("simulate");

    if let Some(opt) = serial_override {
        sim_command.arg("--serial-override").arg(opt);
    }

    sim_command
        .arg("--sel4_arch")
        .arg(test_platform.sel4_arch())
        .arg("--platform")
        .arg(test_platform.platform())
        .arg("-v")
        .current_dir("test-project")
        .env("TEST_CASE", test_case);

    println!(r#"running: TEST_CASE={} {:?}"#, test_case, sim_command);

    let mut sim = spawn_command(sim_command, Some(10000)).expect("Couldn't start simulate command");

    match ready_line_and_func {
        Some((rl, rl_func)) => {
            let mut ready_fired = false;

            loop {
                let line = sim
                    .read_line()
                    .expect("couldn't read line from simulate process");
                println!("{}", line);

                if !ready_fired && rl.is_match(&line) {
                    rl_func();
                    ready_fired = true;
                }

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
        None => loop {
            let line = sim
                .read_line()
                .expect("couldn't read line from simulate process");
            println!("{}", line);

            if pass_line.is_match(&line) {
                sim.process.kill(Signal::SIGKILL).unwrap();
                break;
            }

            if fail_line.is_match(&line) {
                sim.process.kill(Signal::SIGKILL).unwrap();
                panic!("Output line matched failure pattern: {}", line);
            }
        },
    };
}

#[cfg(test)]
mod tests {
    use super::*;

    sequential_test! {
        fn unified_tests_sabre() {
            run_qemu_test::<fn()>(
                "unified_tests",
                Regex::new(".*test result: ok\\. 23 passed;.*").unwrap(),
                Regex::new(".*Root task should never return from main.*").unwrap(),
                None,
                None,
                TestPlatform::SabreAarch32,
            );
        }
    }

    sequential_test! {
        fn unified_tests_virt() {
            run_qemu_test::<fn()>(
                "unified_tests",
                Regex::new(".*test result: ok\\. 23 passed;.*").unwrap(),
                Regex::new(".*Root task should never return from main.*").unwrap(),
                None,
                None,
                TestPlatform::VirtTx1Aarch64,
            );
        }
    }

    sequential_test! {
        fn uart_sabre() {
            use std::net::TcpStream;
            use std::io::Write;

            run_qemu_test(
                "uart",
                Regex::new(".*got byte: 1.*").unwrap(),
                Regex::new(".*Root task should never return from main.*").unwrap(),
                Some((Regex::new(".*thou art ready.*").unwrap(),
                || {
                    let mut stream = TcpStream::connect("localhost:8888").expect("connect stream");
                    stream.write(&[1]).expect("write stream");
                })),
                Some("-serial tcp:localhost:8888,server,nowait,nodelay -serial mon:stdio"),
                TestPlatform::SabreAarch32,
            );
        }
    }
}
