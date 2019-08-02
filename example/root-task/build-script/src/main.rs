use memmap::Mmap;
use selfe_arc;
use std::fs::{self, File};
use std::path::{Path, PathBuf};
use xmas_elf;

#[derive(Debug)]
struct ElfInfo {
    path: PathBuf,
    image_name: String,
    type_name: String,
    required_pages: u32,
    writable_pages: u32,
    required_memory_bits: u32,
    stack_size_bits: u32,
    is_default_stack_size: bool,
}

impl ElfInfo {
    pub fn new(elf_path: &Path, image_name: &str, type_name: &str) -> ElfInfo {
        let file = File::open(elf_path).unwrap();
        let data = unsafe { Mmap::map(&file).unwrap() };
        let elf = xmas_elf::ElfFile::new(data.as_ref()).unwrap();

        let mut required_pages = 0;
        let mut writable_pages = 0;

        // TODO filter for type == Load
        for ph in elf.program_iter() {
            let segment_required_pages = (ph.mem_size() as f64 / 4096.0).ceil() as u32;
            required_pages += segment_required_pages;
            if ph.flags().is_write() {
                writable_pages += segment_required_pages;
            }
        }

        ElfInfo {
            path: elf_path.to_owned(),
            image_name: image_name.to_owned(),
            type_name: type_name.to_owned(),
            required_pages,
            writable_pages,
            required_memory_bits: (writable_pages as f64).log2().ceil() as u32 + 12,
            stack_size_bits: 16,
            is_default_stack_size: true,
        }
    }

    pub fn generate_code(&self) -> String {
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
            self.required_pages,
            self.writable_pages,
            self.required_memory_bits,
            self.stack_size_bits
        )
    }
}

fn archive_and_codegen_elf_binaries<P: AsRef<Path>, I: IntoIterator<Item = ElfInfo>>(
    codegen_path: P,
    elves: I,
) {
    let mut code = "".to_owned();
    let mut arc_params: Vec<(String, PathBuf)> = Vec::new();

    for elf in elves.into_iter() {
        println!("cargo:rerun-if-changed={}", elf.path.display());

        code += &elf.generate_code();
        code += "\n";

        if elf.is_default_stack_size {
            println!(
                "cargo:warning=Using default stack size of 16k for elf process {}",
                elf.image_name
            );
        }

        arc_params.push((elf.image_name, elf.path));
    }

    let p = codegen_path.as_ref();
    let f = fs::write(p, code).expect("Unable to write codegenned elf file");

    selfe_arc::build::link_with_archive(arc_params.iter().map(|(a, b)| (a.as_str(), b.as_path())));
}

fn main() {
    let out_dir = Path::new(&std::env::var_os("OUT_DIR").unwrap()).to_owned();
    let bin_dir = out_dir.join("..").join("..").join("..");

    archive_and_codegen_elf_binaries(
        &out_dir.join("elf_binaries.rs"),
        vec![ElfInfo::new(
            &bin_dir.join("hello-printer"),
            "hello-printer",
            "HelloPrinter",
        )],
    );
}
