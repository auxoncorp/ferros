extern crate selfe_config;
use selfe_config::build_helpers::*;
use selfe_config::model::contextualized::Contextualized;
use selfe_config::model::*;

use std::cmp::max;
use std::env;
use std::fs::File;
use std::io::prelude::*;
use std::path::{Path, PathBuf};

fn main() {
    BuildEnv::request_reruns();
    let config = load_config_from_env_or_default();
    config.print_boolean_feature_flags();
    println!("ferros build.rs config: {:#?}", config);

    let out_dir = PathBuf::from(env::var("OUT_DIR").expect("Required env var OUT_DIR not set"));
    if !out_dir.exists() || !out_dir.is_dir() {
        panic!("OUT_DIR is not an extant directory");
    }
    generate_root_task_stack_types(&out_dir, &config);
    generate_kernel_retype_fan_out_limit_types(&out_dir, &config)
}

fn generate_root_task_stack_types(out_dir: &Path, config: &Contextualized) {
    // TODO - check against target-pointer-width or similar for 32/64 bit
    // differences and panic if unsupported Gleaned from:
    // sel4/kernel/include/arch/arm/arch/32/mode/api/constants.h TODO - instead
    // of calculating these now, we would much rather prefer to have typenum
    // constants generated from the selected headers (e.g. in bindgen, or based
    // on the bindgen output)
    let page_table_bits = 8;
    let pages_per_table = 2u32.pow(page_table_bits);
    let page_bits = 12;
    let bytes_per_page = 2u32.pow(page_bits);
    let bytes_per_page_table = bytes_per_page * pages_per_table;

    let raw_stack_bytes = if let Some(SingleValue::Integer(root_task_stack_bytes)) =
        config.metadata.get("root_task_stack_bytes")
    {
        let bytes = *root_task_stack_bytes;
        if bytes as i128 > ::std::u32::MAX as i128 || bytes <= 0 {
            panic!("root_task_stack_bytes must be greater than 0 and less than u32::MAX");
        } else {
            f64::from(bytes as u32)
        }
    } else {
        const DEFAULT_STACK_BYTES: u32 = 2097152;
        println!(
            "cargo:warning=Using a default root_task_stack_bytes of {}",
            DEFAULT_STACK_BYTES
        );
        f64::from(DEFAULT_STACK_BYTES)
    };
    let stack_reserved_page_tables: usize = max(
        1,
        (raw_stack_bytes / f64::from(bytes_per_page_table)).ceil() as usize,
    );
    let typenum_for_reserved_page_tables_count = format!(
        "pub type RootTaskStackPageTableCount = typenum::U{};",
        stack_reserved_page_tables
    );

    const FILE_NAME: &str = "ROOT_TASK_STACK_PAGE_TABLE_COUNT";
    let mut file = File::create(out_dir.join(FILE_NAME))
        .unwrap_or_else(|_| panic!("Could not create {} file", FILE_NAME));
    file.write_all(typenum_for_reserved_page_tables_count.as_bytes())
        .unwrap_or_else(|_| panic!("Could not write to {}", FILE_NAME));
}
fn generate_kernel_retype_fan_out_limit_types(out_dir: &Path, config: &Contextualized) {
    const FANOUT_PROP: &str = "KernelRetypeFanOutLimit";
    let kernel_retype_fan_out_limit = match config
        .sel4_config
        .get(FANOUT_PROP)
        .unwrap_or_else(|| panic!("Missing required sel4.toml property, {}", FANOUT_PROP))
    {
        SingleValue::Integer(i) => {
            if *i > 0 {
                *i as u32
            } else {
                panic!(
                    "{} sel4.toml property is required to be greater than 0",
                    FANOUT_PROP
                )
            }
        }
        _ => panic!(
            "{} sel4.toml property is required to be a positive integer",
            FANOUT_PROP
        ),
    };
    if !is_typenum_const(kernel_retype_fan_out_limit as u64) {
        panic!("{} sel4.toml property must be an unsigned value supported by `typenum::consts` : (0, 1024], the powers of 2, and the powers of 10.", FANOUT_PROP)
    } else if kernel_retype_fan_out_limit < 16384 {
        // TODO - This is the fan out size of the largest `retype_multi` call in ferros,
        // presently Count == `paging::CodePageCount` in `vspace.rs`
        // If we want to lower the minimum fanout for downstream users,
        // we'll have to split up that `retype_multi` call manually
        panic!(
            "{} sel4.toml property is required to be >= 16384 (2^14)",
            FANOUT_PROP
        )
    }
    let limit_type = format!(
        "pub type {} = typenum::U{};",
        FANOUT_PROP, kernel_retype_fan_out_limit
    );
    const FILE_NAME: &str = "KERNEL_RETYPE_FAN_OUT_LIMIT";
    let mut file = File::create(out_dir.join(FILE_NAME))
        .unwrap_or_else(|_| panic!("Could not create {} file", FILE_NAME));
    file.write_all(limit_type.as_bytes())
        .unwrap_or_else(|_| panic!("Could not write to {}", FILE_NAME));
}

fn is_typenum_const(check: u64) -> bool {
    check.is_power_of_two() || (check == ((check / 10) * 10)) || check <= 1024
}
