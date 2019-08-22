use ferros_build::*;
use std::path::Path;

fn main() {
    let out_dir = Path::new(&std::env::var_os("OUT_DIR").unwrap()).to_owned();
    let bin_dir = out_dir.join("..").join("..").join("..");
    let resources = out_dir.join("resources.rs");

    // let hello = ElfResource {
    //     path: bin_dir.join("hello-printer"),
    //     image_name: "hello-printer".to_owned(),
    //     type_name: "HelloPrinter".to_owned(),
    // };


    let p1 = "/home/mullr/devel/ferros/example/target/aarch64-unknown-linux-gnu/debug/hello_printer-5419dda1c22565ee";
    let p2 = "/home/mullr/devel/ferros/example/target/aarch64-unknown-linux-gnu/debug/foo-ba12d4024785ada4";

    let hello_test = ElfResource {
        path: bin_dir.join(p1),
        image_name: "hello-printer-test".to_owned(),
        type_name: "HelloPrinterTest".to_owned(),
    };

    embed_resources(&resources, vec![&hello_test as &dyn Resource]);
}
