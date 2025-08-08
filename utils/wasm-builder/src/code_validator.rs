// This file is part of Gear.

// Copyright (C) 2022-2025 Gear Technologies Inc.
// SPDX-License-Identifier: GPL-3.0-or-later WITH Classpath-exception-2.0

// This program is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.

// This program is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE. See the
// GNU General Public License for more details.

// You should have received a copy of the GNU General Public License
// along with this program. If not, see <https://www.gnu.org/licenses/>.

use anyhow::{anyhow, bail};
use gear_core::{
    code::{Code, CodeError, ExportError, ImportError},
    gas_metering::Schedule,
};
use gear_wasm_instrument::{Export, ExternalKind, FuncType, Module, SyscallName, TypeRef, ValType};
use std::fmt;
use thiserror::Error;

#[derive(Debug)]
pub struct PrintableFunctionType(String, FuncType);

impl fmt::Display for PrintableFunctionType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let Self(name, func_type) = self;

        let params = PrintableValTypes("param".into(), func_type.params().into());
        let results = PrintableValTypes("result".into(), func_type.results().into());

        write!(f, "(func ${name}{params}{results})")
    }
}

pub struct PrintableValTypes(String, Vec<ValType>);

impl fmt::Display for PrintableValTypes {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let Self(prefix, values) = self;

        let len = values.len();
        if len >= 1 {
            write!(f, " ({prefix} ")?;
            for (val, i) in values.iter().map(|v| PrintableValType(*v)).zip(1_usize..) {
                write!(f, "{val}")?;
                if i != len {
                    write!(f, " ")?;
                }
            }
            write!(f, ")")?;
        }

        Ok(())
    }
}

pub struct PrintableValType(ValType);

impl fmt::Display for PrintableValType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self.0 {
            ValType::I32 => write!(f, "i32"),
            ValType::I64 => write!(f, "i64"),
            ValType::F32 => write!(f, "f32"),
            ValType::F64 => write!(f, "f64"),
            ValType::V128 => write!(f, "v128"),
            ValType::Ref(_) => {
                write!(f, "ref")
            }
        }
    }
}

#[derive(Error, Debug)]
pub enum ExportErrorWithContext {
    #[error("Global index `{_0}` in export `{_1}` is incorrect")]
    IncorrectGlobalIndex(u32, String),
    #[error("Global index `{_0}` in export `{_1}` cannot be mutable")]
    MutableGlobalExport(u32, String),
    #[error("Export `{_0}` references to import `{_1}`")]
    ExportReferencesToImport(String, String),
    #[error(
        "Exported function `{_0}` must have signature `fn {_0}() {{ ... }}:\n\
        Expected signature: `{1}`\n\
        Actual signature: `{2}`"
    )]
    InvalidExportFnSignature(String, PrintableFunctionType, PrintableFunctionType),
    #[error("Excess export of function `{_0}` found")]
    ExcessExport(String),
    #[error("Required export function `init` or `handle` not found")]
    RequiredExportNotFound,
}

impl TryFrom<(Module, ExportError)> for ExportErrorWithContext {
    type Error = anyhow::Error;

    fn try_from((module, export_error): (Module, ExportError)) -> Result<Self, Self::Error> {
        use ExportError::*;

        Ok(match export_error {
            IncorrectGlobalIndex(global_index, export_index) => {
                Self::IncorrectGlobalIndex(global_index, get_export_name(&module, export_index)?)
            }
            MutableGlobalExport(global_index, export_index) => {
                Self::MutableGlobalExport(global_index, get_export_name(&module, export_index)?)
            }
            ExportReferencesToImportFunction(export_index, func_index) => {
                let Some(import_name) = module.import_section.as_ref().and_then(|section| {
                    section
                        .iter()
                        .filter_map(|import| {
                            matches!(import.ty, TypeRef::Func(_)).then_some(import.name.to_string())
                        })
                        .nth(func_index as usize)
                }) else {
                    bail!("failed to get import entry by index");
                };

                Self::ExportReferencesToImport(get_export_name(&module, export_index)?, import_name)
            }
            ExportReferencesToImportGlobal(export_index, global_index) => {
                let Some(import_name) = module.import_section.as_ref().and_then(|section| {
                    section
                        .iter()
                        .filter_map(|import| {
                            matches!(import.ty, TypeRef::Global(_))
                                .then_some(import.name.to_string())
                        })
                        .nth(global_index as usize)
                }) else {
                    bail!("failed to get import entry by index");
                };

                Self::ExportReferencesToImport(get_export_name(&module, export_index)?, import_name)
            }
            InvalidExportFnSignature(export_index) => {
                let export_entry = get_export(&module, export_index)?;

                let ExternalKind::Func = export_entry.kind else {
                    bail!("failed to get export function index");
                };
                let export_func_index = export_entry.index;
                let export_name = export_entry.name.to_string();

                let import_count = module.import_count(|ty| matches!(ty, TypeRef::Func(_))) as u32;
                let real_func_index = export_func_index - import_count;

                let &type_id = module
                    .function_section
                    .as_ref()
                    .and_then(|section| section.get(real_func_index as usize))
                    .ok_or_else(|| anyhow!("failed to get function type"))?;
                let func_type = module
                    .type_section
                    .as_ref()
                    .and_then(|section| section.get(type_id as usize))
                    .ok_or_else(|| anyhow!("failed to get function signature"))?
                    .clone();

                let expected_signature =
                    PrintableFunctionType(export_name.clone(), FuncType::new([], []));
                let actual_signature = PrintableFunctionType(export_name.clone(), func_type);

                Self::InvalidExportFnSignature(export_name, expected_signature, actual_signature)
            }
            ExcessExport(export_index) => {
                Self::ExcessExport(get_export_name(&module, export_index)?)
            }
            RequiredExportNotFound => Self::RequiredExportNotFound,
        })
    }
}

fn get_export_name(module: &Module, export_index: u32) -> anyhow::Result<String> {
    get_export(module, export_index).map(|entry| entry.name.to_string())
}

fn get_export(module: &Module, export_index: u32) -> anyhow::Result<&Export> {
    module
        .export_section
        .as_ref()
        .and_then(|section| section.get(export_index as usize))
        .ok_or_else(|| anyhow!("failed to get export by index"))
}

#[derive(Error, Debug)]
pub enum ImportErrorWithContext {
    #[error("Unknown imported function with index `{0}`")]
    UnknownImport(String),
    #[error("Imported function with index `{0}` is declared multiple times")]
    DuplicateImport(String),
    #[error(
        "Invalid function signature for imported function `{0}`:\n\
        Expected signature: `{1}`\n\
        Actual signature: `{2}`"
    )]
    InvalidImportFnSignature(String, PrintableFunctionType, PrintableFunctionType),
    #[error("Unexpected import `{name}` of kind `{kind}`")]
    UnexpectedImportKind { kind: String, name: String },
}

impl TryFrom<(Module, ImportError)> for ImportErrorWithContext {
    type Error = anyhow::Error;

    fn try_from((module, import_error): (Module, ImportError)) -> Result<Self, Self::Error> {
        use ImportError::*;

        let idx = match import_error {
            UnknownImport(idx)
            | DuplicateImport(idx)
            | InvalidImportFnSignature(idx)
            | UnexpectedImportKind { index: idx, .. } => idx,
        };

        let Some(import_entry) = module
            .import_section
            .as_ref()
            .and_then(|section| section.get(idx as usize))
        else {
            bail!("failed to get import entry by index");
        };

        let import_name = import_entry.name.to_string();

        Ok(match import_error {
            UnknownImport(_) => Self::UnknownImport(import_name),
            DuplicateImport(_) => Self::DuplicateImport(import_name),
            UnexpectedImportKind { kind, .. } => Self::UnexpectedImportKind {
                kind: kind.to_string(),
                name: import_name,
            },
            InvalidImportFnSignature(_) => {
                let syscalls = SyscallName::instrumentable_map();
                let Some(syscall) = syscalls.get(&import_name) else {
                    bail!("failed to get syscall by name");
                };

                let TypeRef::Func(func_index) = import_entry.ty else {
                    bail!("import must be function");
                };

                let expected_signature =
                    PrintableFunctionType(import_name.clone(), syscall.signature().func_type());

                let Some(func_type) = module
                    .type_section
                    .as_ref()
                    .and_then(|section| section.get(func_index as usize).cloned())
                else {
                    bail!("failed to get function type");
                };

                let actual_signature = PrintableFunctionType(import_name.clone(), func_type);

                Self::InvalidImportFnSignature(import_name, expected_signature, actual_signature)
            }
        })
    }
}

#[derive(Error, Debug)]
#[error("code check failed: ")]
pub enum CodeErrorWithContext {
    #[error("Export error: {0}")]
    Export(#[from] ExportErrorWithContext),
    #[error("Import error: {0}")]
    Import(#[from] ImportErrorWithContext),
    Code(#[from] CodeError),
}

impl CodeErrorWithContext {
    fn new(module: Module, error: CodeError) -> Result<Self, anyhow::Error> {
        use CodeError::*;
        match error {
            Validation(_) | Module(_) | Section(_) | Memory(_) | StackEnd(_) | DataSection(_)
            | TypeSection(_) | Instrumentation(_) => Ok(Self::Code(error)),
            Export(error) => {
                let error_with_context: ExportErrorWithContext = (module, error).try_into()?;
                Ok(Self::Export(error_with_context))
            }
            Import(error) => {
                let error_with_context: ImportErrorWithContext = (module, error).try_into()?;
                Ok(Self::Import(error_with_context))
            }
        }
    }
}

/// Validates wasm code in the same way as
/// `pallet_gear::pallet::Pallet::upload_program(...)`.
pub fn validate_program(code: Vec<u8>) -> anyhow::Result<()> {
    let module = Module::new(&code)?;
    let schedule = Schedule::default();
    match Code::try_new(
        code,
        schedule.instruction_weights.version,
        |module| schedule.rules(module),
        schedule.limits.stack_height,
        schedule.limits.data_segments_amount.into(),
        schedule.limits.type_section_len.into(),
        schedule.limits.type_section_params_per_type.into(),
    ) {
        Ok(_) => Ok(()),
        Err(code_error) => Err(CodeErrorWithContext::new(module, code_error)?)?,
    }
}
