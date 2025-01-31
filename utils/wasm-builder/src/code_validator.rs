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

use crate::{DATA_SEGMENTS_AMOUNT_LIMIT, STACK_HEIGHT_LIMIT, TABLE_NUMBER_LIMIT};
use anyhow::{anyhow, bail};
use gear_core::{
    code::{Code, CodeError, ExportError, ImportError, TryNewCodeConfig},
    gas_metering::CustomConstantCostRules,
};
use gear_wasm_instrument::SyscallName;
use pwasm_utils::parity_wasm::{
    self,
    elements::{
        ExportEntry, External, FunctionType, ImportCountType, Internal, Module, Type, ValueType,
    },
};
use std::{error, fmt};
use thiserror::Error;

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
            ExportReferencesToImportFunction(export_index, func_index) => {
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
            ExportReferencesToImportGlobal(export_index, global_index) => {
                let Some(import_name) = module.import_section().and_then(|section| {
                    section
                        .entries()
                        .iter()
                        .filter_map(|import| {
                            matches!(import.external(), External::Global(_))
                                .then_some(import.field().into())
                        })
                        .nth(*global_index as usize)
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
    #[error("Unexpected import `{name}` of kind `{kind}`")]
    UnexpectedImportKind { kind: String, name: String },
}

impl TryFrom<(&Module, &ImportError)> for ImportErrorWithContext {
    type Error = anyhow::Error;

    fn try_from((module, import_error): (&Module, &ImportError)) -> Result<Self, Self::Error> {
        use ImportError::*;

        let idx = match import_error {
            UnknownImport(idx)
            | DuplicateImport(idx)
            | InvalidImportFnSignature(idx)
            | UnexpectedImportKind { index: idx, .. } => idx,
        };

        let Some(import_entry) = module
            .import_section()
            .and_then(|section| section.entries().get(*idx as usize))
        else {
            bail!("failed to get import entry by index");
        };

        let import_name = import_entry.field().to_owned();

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

                let &External::Function(func_index) = import_entry.external() else {
                    bail!("import must be function");
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
        write!(f, "code check failed: ")?;

        match error {
            Validation(_)
            | Codec(_)
            | Section(_)
            | Memory(_)
            | StackEnd(_)
            | DataSection(_)
            | TableSection { .. }
            | Instrumentation(_) => write!(f, "{error}"),
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

impl error::Error for CodeErrorWithContext {}

/// Checks the program code for possible errors.
///
/// NOTE: `pallet-gear` crate performs the same check at the node level
/// when the user tries to upload program code.
pub struct CodeValidator {
    code: Vec<u8>,
    module: Module,
}

impl TryFrom<Vec<u8>> for CodeValidator {
    type Error = anyhow::Error;

    fn try_from(code: Vec<u8>) -> Result<Self, Self::Error> {
        let module: Module = parity_wasm::deserialize_buffer(&code)?;
        Ok(Self { code, module })
    }
}

impl CodeValidator {
    /// Validates wasm code in the same way as
    /// `pallet_gear::pallet::Pallet::upload_program(...)`.
    pub fn validate_program(self) -> anyhow::Result<()> {
        match Code::try_new(
            self.code,
            1,
            |_| CustomConstantCostRules::default(),
            Some(STACK_HEIGHT_LIMIT),
            Some(DATA_SEGMENTS_AMOUNT_LIMIT),
            Some(TABLE_NUMBER_LIMIT),
        ) {
            Err(code_error) => Err(CodeErrorWithContext::from((self.module, code_error)))?,
            _ => Ok(()),
        }
    }

    /// Validate metawasm code in the same way as
    /// `pallet_gear::pallet::Pallet::read_state_using_wasm(...)`.
    pub fn validate_metawasm(self) -> anyhow::Result<()> {
        match Code::try_new_mock_with_rules(
            self.code,
            |_| CustomConstantCostRules::default(),
            TryNewCodeConfig::new_no_exports_check(),
        ) {
            Err(code_error) => Err(CodeErrorWithContext::from((self.module, code_error)))?,
            _ => Ok(()),
        }
    }
}
