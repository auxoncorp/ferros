use selfe_arc;
use selfe_config;
use std::path::Path;
use std::process::{Command, Stdio};

fn main() {
    selfe_config::build_helpers::BuildEnv::request_reruns();

    let build_env = selfe_config::build_helpers::BuildEnv::from_env_vars();
    let config = selfe_config::build_helpers::load_config_from_env_or_default();

    println!("vvvvvvvvvvvvvvvvvvvvvvvvvvvvvvvvvvvvvvvvvvvvvvvvvvvvvvvvvvvvvvvvvvvvvvvvvvvvvvvvv");
    println!("{}", std::env::current_dir().unwrap().display());
    println!("{:#?}", config);
    // println!("{:#?}", config.build.root_task.unwrap().make_command.unwrap());


    let make_root_task_command = config
        .build
        .root_task
        .unwrap()
        .make_command
        .unwrap()
        .to_owned();
    let config_file_dir = build_env.sel4_config_path;

    // Build the subproccess
    let mut build_cmd = Command::new("sh");
    build_cmd
        .arg("-c")
        .arg(&make_root_task_command)
        .current_dir("../hello-printer")
        .env_clear()
        .env("PATH", std::env::var("PATH").unwrap())
        // .current_dir(&config_file_dir)
        // .env("SEL4_CONFIG_PATH", &config_file_path)
        .env("SEL4_PLATFORM", &config.context.platform.to_string())
        .env("SEL4_OVERRIDE_ARCH", &config.context.arch.to_string())
        .env(
            "SEL4_OVERRIDE_SEL4_ARCH",
            &config.context.sel4_arch.to_string(),
        )
        // .env("CARGO_TARGET_DIR", "/home/mullr/devel/ferros/example/target")
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit());

    println!(
        "Running subproc build command:\n   SEL4_PLATFORM={} {}",
        // config_file_dir.map(|p| p.display().to_owned()),
        &config.context.platform,
        &make_root_task_command
    );
    let output = build_cmd.output().expect("Failed to execute build command");

    println!("{:?}", output);
    println!("^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^");

    assert!(output.status.success());

    // TODO: consider stripping the binary before loading it in here

    selfe_arc::build::link_with_archive(vec![(
        "hello-printer",
        Path::new("../hello-printer/target/aarch64-unknown-linux-gnu/debug/hello-printer"),
    )]);
}
