//! Code you might need in a build script for a program built with ferros.

use memmap::Mmap;
use selfe_arc;
use std::fs::{self, File};
use std::path::{Path, PathBuf};
use xmas_elf;

/// A resource that can be embedded in a ferros binary
pub trait Resource {
    fn path(&self) -> &Path;
    /// The name this will get in the embedded selfe-arc
    fn image_name(&self) -> &str;
    fn codegen(&self) -> String;
}

/// A data file resource
pub struct DataResource {
    pub path: PathBuf,
    pub image_name: String,
}

impl Resource for DataResource {
    fn path(&self) -> &Path {
        &self.path
    }

    fn image_name(&self) -> &str {
        &self.image_name
    }

    fn codegen(&self) -> String {
        "".to_owned()
    }
}

/// An elf binary resource. This will generate a struct and an `impl ElfProc`,
/// based on the binary's structure.
pub struct ElfResource {
    pub path: PathBuf,
    /// The name this will get in the embedded selfe-arc
    pub image_name: String,
    /// The name of the generated type for this elf file
    pub type_name: String,
}

impl Resource for ElfResource {
    fn path(&self) -> &Path {
        &self.path
    }

    fn image_name(&self) -> &str {
        &self.image_name
    }

    fn codegen(&self) -> String {
        let file = File::open(&self.path).expect(&format!("ElfResource::codegen: Couldn't open file {}", self.path.display()));
        let data = unsafe { Mmap::map(&file).unwrap() };
        let elf_file = xmas_elf::ElfFile::new(data.as_ref()).unwrap();

        let mut required_pages = 0;
        let mut writable_pages = 0;

        for ph in elf_file
            .program_iter()
            .filter(|h| h.get_type() == Ok(xmas_elf::program::Type::Load))
        {
            let segment_required_pages = (ph.mem_size() as f64 / 4096.0).ceil() as u32;
            required_pages += segment_required_pages;
            if ph.flags().is_write() {
                writable_pages += segment_required_pages;
            }
        }

        let stack_size_bits = 16;
        println!(
            "cargo:warning=Using default stack size of 64k for elf process {}",
            self.image_name
        );

        let required_memory_bits = (writable_pages as f64).log2().ceil() as u32 + 12;

        format!(
            r#"
pub struct {} {{ }}
impl ferros::vspace::ElfProc for {} {{
    const IMAGE_NAME: &'static str = "{}";
    type RequiredPages = typenum::U{};
    type WritablePages = typenum::U{};
    type RequiredMemoryBits = typenum::U{};
    type StackSizeBits = typenum::U{};
}}
"#,
            self.type_name,
            self.type_name,
            self.image_name,
            required_pages,
            writable_pages,
            required_memory_bits,
            stack_size_bits
        )
    }
}

/// Embed the given resources into a selfe-arc. If any code generation is required,
/// put it into the file at `codegen_path`.
pub fn embed_resources<'a, P: AsRef<Path>, I: IntoIterator<Item = &'a dyn Resource>>(
    codegen_path: P,
    resources: I,
) {
    let mut code = "".to_owned();
    let mut arc_params: Vec<(String, PathBuf)> = Vec::new();

    for res in resources.into_iter() {
        code += &res.codegen();
        code += "\n";

        arc_params.push((res.image_name().to_owned(), res.path().to_owned()));
    }

    let p = codegen_path.as_ref();
    let _f = fs::write(p, code).expect("Unable to write generated code for resources");

    selfe_arc::build::link_with_archive(arc_params.iter().map(|(a, b)| (a.as_str(), b.as_path())));
}
