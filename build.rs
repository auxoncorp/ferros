extern crate confignoble;
use confignoble::build_helpers::*;

use fel4_config::{
    get_fel4_config, infer_manifest_location_from_env, infer_support_extension_from_env,
    BuildProfile, Fel4Config, FlatTomlValue, ManifestDiscoveryError, SupportedTarget,
};

use std::cmp::max;
use std::env;
use std::fs::{self, File};
use std::io::prelude::*;
use std::path::{Path, PathBuf};
use std::str::FromStr;

fn main() {
    BuildEnv::request_reruns();
    let config = load_config_from_env_or_default();
    config.print_boolean_feature_flags();

    let (fel4_manifest_path, profile) = match infer_manifest_location_from_env() {
        Ok(v) => v,
        Err(e) => {
            if e == ManifestDiscoveryError::MissingEnvVar("FEL4_MANIFEST_PATH".to_owned()) {
                // No FEL4_MANIFEST_PATH provided suggests we're building `ferros` as a standalone
                // lib or outside the context of a feL4 project. Use the local fel4.toml default.
                let manifest_dir = PathBuf::from(
                    env::var("CARGO_MANIFEST_DIR")
                        .expect("Required env var CARGO_MANIFEST_DIR not set"),
                );
                let fel4_manifest_path = manifest_dir.join("fel4.toml");
                let profile = BuildProfile::from_str(
                    &env::var("PROFILE").expect("Required env var PROFILE not set"),
                )
                .expect("Failed to interpret PROFILE env var as a valid fel4_config::BuildProfile");
                (fel4_manifest_path, profile)
            } else {
                panic!("{:?}", e)
            }
        }
    };
    print_rerun_flags(&fel4_manifest_path);
    if !fel4_manifest_path.exists() {
        panic!("Required fel4.toml file missing.");
    }
    let support_ext = infer_support_extension_from_env();
    let config = get_fel4_config(fel4_manifest_path, &profile, &support_ext)
        .expect("ferros build failure in manifest parsing");

    let out_dir = PathBuf::from(env::var("OUT_DIR").expect("Required env var OUT_DIR not set"));
    if !out_dir.exists() || !out_dir.is_dir() {
        panic!("OUT_DIR is not an extant directory");
    }
    generate_root_task_stack_types(&out_dir, &config);
    generate_kernel_retype_fan_out_limit_types(&out_dir, &config)
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
fn generate_kernel_retype_fan_out_limit_types(out_dir: &Path, config: &Fel4Config) {
    const PROPERTY: &'static str = "KernelRetypeFanOutLimit";
    let kernel_retype_fan_out_limit = match config.properties.get(PROPERTY).expect(&format!(
        "Missing required fel4.toml property, {}",
        PROPERTY
    )) {
        FlatTomlValue::Integer(i) => {
            if *i > 0 {
                *i as u32
            } else {
                panic!(
                    "{} fel4.toml property is required to be greater than 0",
                    PROPERTY
                )
            }
        }
        _ => panic!(
            "{} fel4.toml property is required to be a positive integer",
            PROPERTY
        ),
    };
    if !is_typenum_const(kernel_retype_fan_out_limit as u64) {
        panic!("{} fel4.toml property must be an unsigned value supported by `typenum::consts` : (0, 1024], the powers of 2, and the powers of 10.", PROPERTY)
    } else if kernel_retype_fan_out_limit < 16384 {
        // TODO - This is the fan out size of the largest `retype_multi` call in ferros,
        // presently Count == `paging::CodePageCount` in `vspace.rs`
        // If we want to lower the minimum fanout for downstream users,
        // we'll have to split up that `retype_multi` call manually
        panic!(
            "{} fel4.toml property is required to be >= 16384 (2^14)",
            PROPERTY
        )
    }
    let limit_type = format!(
        "pub type {} = typenum::U{};",
        PROPERTY, kernel_retype_fan_out_limit
    );
    const FILE_NAME: &'static str = "KERNEL_RETYPE_FAN_OUT_LIMIT";
    let mut file = File::create(out_dir.join(FILE_NAME))
        .expect(&format!("Could not create {} file", FILE_NAME));
    file.write_all(limit_type.as_bytes())
        .expect(&format!("Could not write to {}", FILE_NAME))
}

fn is_typenum_const(check: u64) -> bool {
    check.is_power_of_two() || (check == ((check / 10) * 10)) || check <= 1024
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
