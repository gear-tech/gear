// This file is part of Gear.

// Copyright (C) 2022-2024 Gear Technologies Inc.
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
use gear_core::code::{CodeError, ExportError, ImportError};
use gear_wasm_instrument::SyscallName;
use pwasm_utils::parity_wasm::elements::{
    ExportEntry, External, FunctionType, ImportCountType, Internal, Module, Type, ValueType,
};
use std::{fmt, path::PathBuf};
use thiserror::Error;

/// Errors than can occur when building.
#[derive(Error, Debug)]
pub enum BuilderError {
    #[error("invalid manifest path `{0}`")]
    ManifestPathInvalid(PathBuf),

    #[error("please add \"rlib\" to [lib.crate-type]")]
    CrateTypeInvalid,

    #[error("cargo command run failed: {0}")]
    CargoRunFailed(String),

    #[error("unable to find the root package in cargo metadata")]
    RootPackageNotFound,

    #[error("code check failed: {0}")]
    CodeCheckFailed(CodeErrorWithContext),

    #[error("cargo toolchain is invalid `{0}`")]
    CargoToolchainInvalid(String),

    #[error(
        "recommended toolchain `{0}` not found, install it using the command:\n\
        rustup toolchain install {0} --component llvm-tools --target wasm32-unknown-unknown\n\n\
        after installation, do not forget to set `channel = \"{0}\"` in `rust-toolchain.toml` file"
    )]
    RecommendedToolchainNotFound(String),
}

#[derive(Debug)]
pub struct PrintableFunctionType(String, FunctionType);

impl fmt::Display for PrintableFunctionType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let Self(name, func_type) = self;

        let params = PrintableValueTypes("param".into(), func_type.params().into());
        let results = PrintableValueTypes("result".into(), func_type.results().into());

        write!(f, "(func ${name}{params}{results})")
    }
}

pub struct PrintableValueTypes(String, Vec<ValueType>);

impl fmt::Display for PrintableValueTypes {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let Self(prefix, values) = self;

        let len = values.len();
        if len >= 1 {
            write!(f, " ({prefix} ")?;
            for (val, i) in values.iter().map(|v| PrintableValueType(*v)).zip(1_usize..) {
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

pub struct PrintableValueType(ValueType);

impl fmt::Display for PrintableValueType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self.0 {
            ValueType::I32 => write!(f, "i32"),
            ValueType::I64 => write!(f, "i64"),
            ValueType::F32 => write!(f, "f32"),
            ValueType::F64 => write!(f, "f64"),
        }
    }
}

#[derive(Error, Debug)]
pub enum ExportErrorWithContext {
    #[error("Global index `{_0}` in export `{_1}` is incorrect")]
    IncorrectGlobalIndex(u32, String),
    #[error("Global index `{_0}` in export `{_1}` cannot be mutable")]
    MutableGlobalExport(u32, String),
    #[error("Export `{_0}` references to imported function `{_1}`")]
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

impl TryFrom<(&Module, &ExportError)> for ExportErrorWithContext {
    type Error = anyhow::Error;

    fn try_from((module, export_error): (&Module, &ExportError)) -> Result<Self, Self::Error> {
        use ExportError::*;

        Ok(match export_error {
            IncorrectGlobalIndex(global_index, export_index) => {
                Self::IncorrectGlobalIndex(*global_index, get_export_name(module, *export_index)?)
            }
            MutableGlobalExport(global_index, export_index) => {
                Self::MutableGlobalExport(*global_index, get_export_name(module, *export_index)?)
            }
            ExportReferencesToImport(export_index, func_index) => {
                let Some(import_name) = module.import_section().and_then(|section| {
                    section
                        .entries()
                        .iter()
                        .filter_map(|import| {
                            matches!(import.external(), External::Function(_))
                                .then_some(import.field().into())
                        })
                        .nth(*func_index as usize)
                }) else {
                    bail!("failed to get import entry by index");
                };

                Self::ExportReferencesToImport(get_export_name(module, *export_index)?, import_name)
            }
            InvalidExportFnSignature(export_index) => {
                let export_entry = get_export(module, *export_index)?;

                let &Internal::Function(export_func_index) = export_entry.internal() else {
                    bail!("failed to get export function index");
                };
                let export_name = export_entry.field().to_owned();

                let import_count = module.import_count(ImportCountType::Function) as u32;
                let real_func_index = export_func_index - import_count;

                let type_id = module
                    .function_section()
                    .and_then(|section| section.entries().get(real_func_index as usize))
                    .ok_or_else(|| anyhow!("failed to get function type"))?
                    .type_ref();
                let Type::Function(func_type) = module
                    .type_section()
                    .and_then(|section| section.types().get(type_id as usize))
                    .ok_or_else(|| anyhow!("failed to get function signature"))?
                    .clone();

                let expected_signature =
                    PrintableFunctionType(export_name.clone(), FunctionType::default());
                let actual_signature = PrintableFunctionType(export_name.clone(), func_type);

                Self::InvalidExportFnSignature(export_name, expected_signature, actual_signature)
            }
            ExcessExport(export_index) => {
                Self::ExcessExport(get_export_name(module, *export_index)?)
            }
            RequiredExportNotFound => Self::RequiredExportNotFound,
        })
    }
}

fn get_export_name(module: &Module, export_index: u32) -> anyhow::Result<String> {
    get_export(module, export_index).map(|entry| entry.field().into())
}

fn get_export(module: &Module, export_index: u32) -> anyhow::Result<&ExportEntry> {
    module
        .export_section()
        .and_then(|section| section.entries().get(export_index as usize))
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
}

impl TryFrom<(&Module, &ImportError)> for ImportErrorWithContext {
    type Error = anyhow::Error;

    fn try_from((module, import_error): (&Module, &ImportError)) -> Result<Self, Self::Error> {
        use ImportError::*;

        let (UnknownImport(idx) | DuplicateImport(idx) | InvalidImportFnSignature(idx)) =
            import_error;

        let Some(import_entry) = module
            .import_section()
            .and_then(|section| section.entries().iter().nth(*idx as usize))
        else {
            bail!("failed to get import entry by index");
        };

        let &External::Function(func_index) = import_entry.external() else {
            bail!("import must be function");
        };

        let import_name = import_entry.field().to_owned();

        Ok(match import_error {
            UnknownImport(_) => Self::UnknownImport(import_name),
            DuplicateImport(_) => Self::DuplicateImport(import_name),
            InvalidImportFnSignature(_) => {
                let syscalls = SyscallName::instrumentable_map();
                let Some(syscall) = syscalls.get(&import_name) else {
                    bail!("failed to get syscall by name");
                };

                let expected_signature =
                    PrintableFunctionType(import_name.clone(), syscall.signature().func_type());

                let Some(Type::Function(func_type)) = module
                    .type_section()
                    .and_then(|section| section.types().get(func_index as usize).cloned())
                else {
                    bail!("failed to get function type");
                };

                let actual_signature = PrintableFunctionType(import_name.clone(), func_type);

                Self::InvalidImportFnSignature(import_name, expected_signature, actual_signature)
            }
        })
    }
}

#[derive(Debug)]
pub struct CodeErrorWithContext(Module, CodeError);

impl From<(Module, CodeError)> for CodeErrorWithContext {
    fn from((module, error): (Module, CodeError)) -> Self {
        Self(module, error)
    }
}

impl fmt::Display for CodeErrorWithContext {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        use CodeError::*;

        let Self(module, error) = self;

        match error {
            Validation | Decode | Encode | Section(_) | Memory(_) | StackEnd(_)
            | Initialization(_) | Instrumentation(_) => write!(f, "{error}"),
            Export(error) => {
                let error_with_context: ExportErrorWithContext =
                    (module, error).try_into().map_err(|_| fmt::Error)?;
                write!(f, "Export error: {error_with_context}")
            }
            Import(error) => {
                let error_with_context: ImportErrorWithContext =
                    (module, error).try_into().map_err(|_| fmt::Error)?;
                write!(f, "Import error: {error_with_context}")
            }
        }
    }
}
