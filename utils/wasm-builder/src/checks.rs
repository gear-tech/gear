use gear_wasm_instrument::parity_wasm::elements::{Internal, Module, Type};

#[derive(Debug)]
pub enum Error {
    ExportSectionMissing,
    ImportSectionMissing,
    FunctionSectionMissing,
    TypeSectionMissing,
    CodeSectionMissing,
    UnexpectedFunctionExport(String),
    MissingEntryFunction,
    InvalidExportFunctionSignature,
}

pub fn do_checks(module: Module) -> Result<(), Error> {

    let exports = module.export_section().ok_or(Error::ExportSectionMissing)?;
    let imports = module.import_section().ok_or(Error::ImportSectionMissing)?;
    let functions = module.function_section().ok_or(Error::FunctionSectionMissing)?;
    let types = module.type_section().ok_or(Error::TypeSectionMissing)?;
    let code = module.code_section().ok_or(Error::CodeSectionMissing)?;

    let imported_functions = imports.functions();

    // While this is checked in Code::new, we might as well check it when building wasm program
    let mut entry = false;
    for e in exports.entries() {
        if let Internal::Function(i) = e.internal() {
            match e.field() {
                "init" | "handle" | "handle_reply" | "handle_signal" | "state" | "meta" => {
                    entry = true;
                    let Type::Function(ref t) = types.types()[functions.entries()[*i as usize - imported_functions]];
                    if !t.params().is_empty() || !t.results().is_empty() {
                        return Err(Error::InvalidExportFunctionSignature);
                    }
                },
                _ => return Err(Error::UnexpectedFunctionExport(e.field().to_string()))
            }
        }
    }
    if !entry {
        return Err(Error::MissingEntryFunction)
    }



    Ok(())
}