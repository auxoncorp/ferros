#[cfg(not(RealBuild))]
fn main() {
    cargo_5730::run_build_script();
}


#[cfg(RealBuild)]
fn main() {
    use ferros_build::*;
    use std::path::Path;

    let out_dir = Path::new(&std::env::var_os("OUT_DIR").unwrap()).to_owned();
    let bin_dir = out_dir.join("..").join("..").join("..");
    let resources = out_dir.join("resources.rs");

    let hello = ElfResource {
        path: bin_dir.join("hello-printer"),
        image_name: "hello-printer".to_owned(),
        type_name: "HelloPrinter".to_owned(),
    };

    embed_resources(&resources, vec![&hello as &dyn Resource]);
}
