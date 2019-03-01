use fel4_config::{
    get_fel4_config, infer_support_extension_from_env, BuildProfile, FullFel4Manifest,
    SupportExtension, SupportedTarget,
};

use fel4_config::Executable;
use fel4_config::Fel4Config;
use std::cmp::max;
use std::env;
use std::fs::{self, File};
use std::io::prelude::*;
use std::path::Path;
use std::path::PathBuf;
use std::str::FromStr;

fn main() {
    let manifest_dir = PathBuf::from(
        env::var("CARGO_MANIFEST_DIR").expect("Required env var CARGO_MANIFEST_DIR not set"),
    );
    let out_dir = PathBuf::from(env::var("OUT_DIR").expect("Required env var OUT_DIR not set"));
    let fel4_manifest_path = manifest_dir.join("fel4.toml");
    print_rerun_flags(&fel4_manifest_path);
    if !fel4_manifest_path.exists() {
        panic!("Required fel4.toml file missing.");
    }
    let profile =
        BuildProfile::from_str(&env::var("PROFILE").expect("Required env var PROFILE not set"))
            .expect("Failed to interpret PROFILE env var as a valid fel4_config::BuildProfile");
    let support_ext = infer_support_extension_from_env();
    let config = get_fel4_config(fel4_manifest_path, &profile, &support_ext)
        .expect("ferros build failure in manifest parsing");

    if !out_dir.exists() || !out_dir.is_dir() {
        panic!("OUT_DIR is not an extant directory");
    }
    generate_root_task_stack_types(&out_dir, &config)
}

fn generate_root_task_stack_types(out_dir: &Path, config: &Fel4Config) {
    let is_armlike = match &config.target {
        SupportedTarget::X8664Sel4Fel4 => false,
        SupportedTarget::Armv7Sel4Fel4 => true,
        SupportedTarget::Aarch64Sel4Fel4 => true,
        SupportedTarget::Custom(c) => {
            c.full_name().starts_with("arm") || c.full_name().starts_with("aarch")
        }
    };
    if !is_armlike {
        panic!("ferros is not yet portable across architectures")
    }
    // TODO - check against target-pointer-width or similar for 32/64 bit differences and panic if unsupported

    // Gleaned from: sel4/kernel/include/arch/arm/arch/32/mode/api/constants.h
    let page_table_bits = 8;
    let pages_per_table = 2u32.pow(page_table_bits);
    let page_bits = 12;
    let bytes_per_page = 2u32.pow(page_bits);
    let bytes_per_page_table = bytes_per_page * pages_per_table;
    let stack_reserved_page_tables: usize = max(
        1,
        (f64::from(config.executable.root_task_stack_bytes) / f64::from(bytes_per_page_table))
            .ceil() as usize,
    );
    let typenum_for_reserved_page_tables_count = format!(
        "pub type RootTaskStackPageTableCount = typenum::U{};",
        stack_reserved_page_tables
    );

    const FILE_NAME: &'static str = "ROOT_TASK_STACK_PAGE_TABLE_COUNT";
    let mut file = File::create(out_dir.join(FILE_NAME))
        .expect(&format!("Could not create {} file", FILE_NAME));
    file.write_all(typenum_for_reserved_page_tables_count.as_bytes())
        .expect(&format!("Could not write to {}", FILE_NAME))
}

fn print_rerun_flags(fel4_manifest_path: &Path) {
    println!(
        "cargo:rerun-if-changed={}",
        fs::canonicalize(&fel4_manifest_path)
            .expect("Could not canonicalize the fel4 manifest path")
            .display()
    );
    println!("cargo:rerun-if-env-changed=PROFILE");
    println!("cargo:rerun-if-env-changed=OUT_DIR");
    println!("cargo:rerun-if-env-changed=FEL4_CUSTOM_TARGET_PLATFORM_PAIRS");
}
