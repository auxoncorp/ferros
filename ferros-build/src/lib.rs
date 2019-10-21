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
    /// Explicitly specify the process stack size
    pub stack_size_bits: Option<u8>,
}

/// Format n as a fully expanded typenum (in binary form), so allowing arbitrary
/// numbers to be specified.
fn format_as_typenum(n: u64) -> String {
    if n == 0 {
        "typenum::UTerm".to_string()
    } else if (n % 2) == 1 {
        "typenum::UInt<".to_owned() + &format_as_typenum(n >> 1) + ", typenum::B1>"
    } else {
        "typenum::UInt<".to_owned() + &format_as_typenum(n >> 1) + ", typenum::B0>"
    }
}

fn round_down_to_page_boundary(addr: u64) -> u64 {
    addr & !0xfff
}

fn round_up_to_page_boundary(addr: u64) -> u64 {
    if addr & 0xfff == 0 {
        addr
    } else {
        (addr + 0x1000) & !0xfff
    }
}

impl Resource for ElfResource {
    fn path(&self) -> &Path {
        &self.path
    }

    fn image_name(&self) -> &str {
        &self.image_name
    }

    fn codegen(&self) -> String {
        let file = File::open(&self.path).expect(&format!(
            "ElfResource::codegen: Couldn't open file {}",
            self.path.display()
        ));
        let data = unsafe { Mmap::map(&file).unwrap() };
        let elf_file = xmas_elf::ElfFile::new(data.as_ref()).unwrap();

        let mut read_only_pages = 0;
        let mut writable_pages = 0;

        for ph in elf_file
            .program_iter()
            .filter(|h| h.get_type() == Ok(xmas_elf::program::Type::Load))
        {
            let page_aligned_segment_size =
                round_up_to_page_boundary(ph.virtual_addr() + ph.mem_size())
                    - round_down_to_page_boundary(ph.virtual_addr());
            let segment_required_pages = page_aligned_segment_size >> 12;
            if ph.flags().is_write() {
                writable_pages += segment_required_pages;
            } else {
                read_only_pages += segment_required_pages;
            }
        }

        let stack_size_bits = self
            .stack_size_bits
            .map(|ssb| ssb as u64)
            .unwrap_or_else(|| {
                println!(
                    "cargo:warning=Using default stack size of 64k for elf process {}",
                    self.image_name
                );
                16u64
            });

        let required_memory_bits = (writable_pages as f64).log2().ceil() as u32 + 12;
        let required_pages = (1 << (required_memory_bits - 12)) + read_only_pages;

        format!(
            r#"
pub struct {} {{ }}
impl ferros::vspace::ElfProc for {} {{
    const IMAGE_NAME: &'static str = "{}";
    type RequiredPages = {};
    type WritablePages = {};
    type RequiredMemoryBits = {};
    type StackSizeBits = {};
}}
"#,
            self.type_name,
            self.type_name,
            self.image_name,
            format_as_typenum(required_pages),
            format_as_typenum(writable_pages),
            format_as_typenum(required_memory_bits.into()),
            format_as_typenum(stack_size_bits)
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

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn test_format_as_typenum() {
        assert_eq!(format_as_typenum(0), "typenum::UTerm".to_string());
        assert_eq!(
            format_as_typenum(1),
            "typenum::UInt<typenum::UTerm, typenum::B1>".to_string()
        );
        assert_eq!(
            format_as_typenum(2),
            "typenum::UInt<typenum::UInt<typenum::UTerm, typenum::B1>, typenum::B0>".to_string()
        );
        assert_eq!(
            format_as_typenum(3),
            "typenum::UInt<typenum::UInt<typenum::UTerm, typenum::B1>, typenum::B1>".to_string()
        );
        assert_eq!(format_as_typenum(4), "typenum::UInt<typenum::UInt<typenum::UInt<typenum::UTerm, typenum::B1>, typenum::B0>, typenum::B0>".to_string());
    }

}
