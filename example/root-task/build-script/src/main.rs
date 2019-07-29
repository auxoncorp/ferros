use selfe_arc;
use std::path::{Path, PathBuf};

fn main() {
    // TODO: consider stripping the binary before loading it in here

    let out_dir = Path::new(&std::env::var_os("OUT_DIR").unwrap()).to_owned();
    let bin_dir = out_dir.join("..").join("..").join("..");

    let hello : PathBuf = bin_dir.join("hello-printer").to_owned();

    selfe_arc::build::link_with_archive(vec![(
        "hello-printer", hello.as_path()
    )]);
}
