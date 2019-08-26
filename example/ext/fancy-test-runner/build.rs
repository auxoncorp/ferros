#[cfg(not(RealBuild))]
fn main() {
    cargo_5730::run_build_script();
}

#[cfg(RealBuild)]
fn main() {
    use ferros_build::*;
    use regex::Regex;
    use std::fs::{self, DirEntry};
    use std::path::Path;
    use std::vec::Vec;

    let out_dir = Path::new(&std::env::var_os("OUT_DIR").unwrap()).to_owned();
    let resources_rs = out_dir.join("resources.rs");

    let bin_dir = out_dir.join("..").join("..").join("..");

    let mut rs = vec!();

    // test executable names end in "-<16 hex digits>"
    let test_filename_regex = Regex::new(".*\\-[0-9a-f]{16}$").unwrap();

    println!("cargo:rerun-if-changed={}", bin_dir.display());

    for entry in fs::read_dir(bin_dir).unwrap() {
        let entry = entry.unwrap();
        let path = entry.path();
        let file_name = path.file_name().unwrap().to_str().unwrap();
        if entry.file_type().unwrap().is_file() && test_filename_regex.is_match(file_name) {
            rs.push(DataResource {
                    path: path.to_owned(),
                    image_name: file_name.to_owned(),
                });

            println!("cargo:rerun-if-changed={}", path.display());
        }
    }

    let mut resources : Vec<&dyn Resource> = vec![];
    for r in rs.iter() {
        resources.push(r as &dyn Resource);
    }

    embed_resources(
        &resources_rs,
        resources,
    );
}
