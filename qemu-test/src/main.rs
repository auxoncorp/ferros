extern crate regex;
extern crate rexpect;

fn main() {}

#[cfg(test)]
mod tests {
    use regex::Regex;
    use rexpect::errors::*;
    use rexpect::session::spawn_command;
    use std::process::Command;

    fn run_qemu_test(name: &str, pass_line: Regex, fail_line: Regex) {
        println!("running 'TEST_CASE={} cargo fel4 build", name);

        Command::new("cargo")
            .arg("fel4")
            .arg("build")
            .current_dir("fel4-test-project")
            .env("TEST_CASE", name)
            .output()
            .expect("Couldn't build test project");

        println!("running 'cargo fel4 simulate");

        let mut sim_command = Command::new("cargo");
        sim_command
            .arg("fel4")
            .arg("simulate")
            .current_dir("fel4-test-project");

        let mut sim =
            spawn_command(sim_command, Some(10000)).expect("Couldn't start simulate command");

        loop {
            let line = sim
                .read_line()
                .expect("couldn't read line from simulate process");

            if pass_line.is_match(&line) {
                break;
            }

            if fail_line.is_match(&line) {
                panic!("Output line matched failure pattern: {}", line);
            }
        }
    }

    #[test]
    fn test_root_task_runs() {
        run_qemu_test(
            "root_task_runs",
            Regex::new(".*hello from the root task.*").unwrap(),
            Regex::new(".*Root task should never return from main.*").unwrap(),
        );
    }
}
