use parity_wasm::elements::{Module, Section, Serialize};
use std::{fs, path::PathBuf};
use structopt::StructOpt;

/// Parse info of wasm binaries
#[derive(Debug, StructOpt)]
pub struct Info {
    /// Show code hex
    #[structopt(short, long)]
    pub hex: bool,

    /// Show code size
    #[structopt(long)]
    pub size: bool,

    /// Strip custom sections
    #[structopt(short, long)]
    pub strip_custom_sections: bool,

    /// Path or hex encoding of wasm binary
    pub wasm: String,
}

impl Info {
    /// Parse wasm module from wasm code or path
    pub fn module(&self) -> Module {
        let mut is_hex = false;
        let code = if PathBuf::from(&self.wasm).exists() {
            fs::read(&self.wasm).expect("File not exists")
        } else {
            is_hex = true;
            let hex = self.wasm.trim_start_matches("0x");
            hex::decode(hex).expect("Decode hex failed")
        };

        if self.size {
            println!("Code size: {}", code.len());
        }

        if self.hex && !is_hex {
            println!("Code hex: {}", hex::encode(&code));
        }

        parity_wasm::deserialize_buffer(&code).expect("Parse wasm module failed")
    }

    /// Strip custom sections from wasm module
    pub fn strip_custom_sections(module: &mut Module) {
        module
            .sections_mut()
            .retain(|section| !matches!(section, Section::Reloc(_) | Section::Custom(_)))
    }
}

fn main() {
    let info = Info::from_args();

    let mut module = info.module();
    println!("Module sections: {}", module.sections().len());

    for section in module.sections() {
        match *section {
            Section::Import(ref import_section) => {
                println!("\tImports: {}", import_section.entries().len());
                import_section
                    .entries()
                    .iter()
                    .map(|e| println!("\t\t{}.{}", e.module(), e.field()))
                    .count();
            }
            Section::Export(ref exports_section) => {
                println!("\tExports: {}", exports_section.entries().len());
                exports_section
                    .entries()
                    .iter()
                    .map(|e| println!("\t\t{}", e.field()))
                    .count();
            }
            Section::Function(ref function_section) => {
                println!("\tFunctions: {}", function_section.entries().len());
            }
            Section::Type(ref type_section) => {
                println!("\tTypes: {}", type_section.types().len());
            }
            Section::Global(ref globals_section) => {
                println!("\tGlobals: {}", globals_section.entries().len());
            }
            Section::Table(ref table_section) => {
                println!("\tTables: {}", table_section.entries().len());
            }
            Section::Memory(ref memory_section) => {
                println!("\tMemories: {}", memory_section.entries().len());
            }
            Section::Custom(ref custom_section) => {
                println!("\tCustom: {}", custom_section.name());
            }
            Section::Data(ref data_section) if !data_section.entries().is_empty() => {
                let data = &data_section.entries()[0];
                println!("\tData size: {}", data.value().len());
            }
            _ => {}
        }
    }

    if info.strip_custom_sections {
        Info::strip_custom_sections(&mut module);
        let mut code = vec![];
        module.serialize(&mut code).expect("Serialize code failed");
        println!(
            "Code after stripping custom sections: {}",
            hex::encode(code)
        );
    }
}
