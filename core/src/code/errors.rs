// This file is part of Gear.

// Copyright (C) 2024-2025 Gear Technologies Inc.
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

//! Module that describes various code errors.

pub use gear_wasm_instrument::{InstrumentationError, ModuleError};
pub use wasmparser::BinaryReaderError;

/// Section name in WASM module.
#[derive(PartialEq, Eq, Debug, derive_more::Display)]
pub enum SectionName {
    /// Type section.
    #[display("Type section")]
    Type,
    /// Import section.
    #[display("Import section")]
    Import,
    /// Function (Code) section.
    #[display("Function section")]
    Function,
    /// Data section.
    #[display("Data section")]
    Data,
    /// Global section.
    #[display("Global section")]
    Global,
    /// Table section.
    #[display("Table section")]
    Table,
    /// Element section.
    #[display("Element section")]
    Element,
    /// Export section.
    #[display("Export section")]
    Export,
    /// Start section.
    #[display("Start section")]
    Start,
}

/// Section error in WASM module.
#[derive(Debug, derive_more::Display, PartialEq, Eq)]
pub enum SectionError {
    /// Section not found.
    #[display("{_0} not found")]
    NotFound(SectionName),
    /// Section not supported.
    #[display("{_0} not supported")]
    NotSupported(SectionName),
}

/// Memory error in WASM module.
#[derive(Debug, derive_more::Display, PartialEq, Eq)]
pub enum MemoryError {
    /// Memory entry not found in import section.
    #[display("Memory entry not found")]
    EntryNotFound,
    /// The WASM module has invalid count of static memory pages.
    #[display("The WASM module has invalid count of static memory pages")]
    InvalidStaticPageCount,
}

/// Stack end error in WASM module.
#[derive(Debug, derive_more::Display, PartialEq, Eq)]
pub enum StackEndError {
    /// Unsupported initialization of gear stack end global variable.
    #[display("Unsupported initialization of gear stack end global")]
    Initialization,
    /// Gear stack end offset is not aligned to wasm page size.
    #[display("Gear stack end {_0:#x} is not aligned to wasm page size")]
    NotAligned(u32),
    /// Gear stack end is out of static memory.
    #[display("Gear stack end {_0:#x} is out of static memory 0x0..{_1:#x}")]
    OutOfStatic(u32, u64),
}

/// Data section error in WASM module.
#[derive(Debug, derive_more::Display, PartialEq, Eq)]
pub enum DataSectionError {
    /// Unsupported initialization of data segment.
    #[display("Unsupported initialization of data segment")]
    Initialization,
    /// Data section overlaps gear stack.
    #[display("Data segment {_0:#x} overlaps gear stack 0x0..{_1:#x}")]
    GearStackOverlaps(u32, u32),
    /// Data segment end address is out of possible 32 bits address space.
    #[display("Data segment {_0:#x} ends out of possible 32 bits address space")]
    EndAddressOverflow(u32),
    /// Data segment end address is out of static memory.
    #[display("Data segment {_0:#x}..={_1:#x} is out of static memory 0x0..{_2:#x}")]
    EndAddressOutOfStaticMemory(u32, u32, u64),
    /// Data segment amount exceeds the limit.
    #[display("Data segment amount limit exceeded: limit={limit}, actual={actual}")]
    DataSegmentsAmountLimit {
        /// Limit of data segments.
        limit: u32,
        /// Actual amount of data segments.
        actual: u32,
    },
}

/// Type section error in WASM module.
#[derive(Debug, derive_more::Display, PartialEq, Eq)]
pub enum TypeSectionError {
    /// Type section length exceeds the limit.
    #[display("Type section length limit exceeded: limit={limit}, actual={actual}")]
    LengthLimitExceeded {
        /// Max length of type section.
        limit: u32,
        /// Actual length of type section.
        actual: u32,
    },
    /// Type section number of parameters per type exceeds the limit.
    #[display("Type section parameters per type limit exceeded: limit={limit}, actual={actual}")]
    ParametersPerTypeLimitExceeded {
        /// Max number of parameters per type.
        limit: u32,
        /// Actual number of parameters per type.
        actual: u32,
    },
}

/// Export error in WASM module.
#[derive(Debug, derive_more::Display, PartialEq, Eq)]
pub enum ExportError {
    /// Incorrect global export index. Can occur when export refers to not existing global index.
    #[display("Global index `{_0}` in export index `{_1}` is incorrect")]
    IncorrectGlobalIndex(u32, u32),
    /// Exporting mutable globals is restricted by the Gear protocol.
    #[display("Global index `{_0}` in export index `{_1}` cannot be mutable")]
    MutableGlobalExport(u32, u32),
    /// Export references to an import function, which is not allowed.
    #[display("Export index `{_0}` references to imported function with index `{_1}`")]
    ExportReferencesToImportFunction(u32, u32),
    /// Export references to an import global, which is not allowed.
    #[display("Export index `{_0}` references to imported global with index `{_1}`")]
    ExportReferencesToImportGlobal(u32, u32),
    /// The signature of an exported function is invalid.
    #[display("Exported function with index `{_0}` must have signature `fn f() {{ ... }}`")]
    InvalidExportFnSignature(u32),
    /// The provided code contains excess function export.
    #[display("Excess export with index `{_0}` found")]
    ExcessExport(u32),
    /// The provided code doesn't contain the required `init` or `handle` export function.
    #[display("Required export function `init` or `handle` is not found")]
    RequiredExportNotFound,
}

/// Import error in WASM module.
#[derive(Debug, derive_more::Display, PartialEq, Eq)]
pub enum ImportError {
    /// The imported function is not supported by the Gear protocol.
    #[display("Unknown imported function with index `{_0}`")]
    UnknownImport(u32),
    /// The imported function is declared multiple times.
    #[display("Imported function with index `{_0}` is declared multiple times")]
    DuplicateImport(u32),
    /// The signature of an imported function is invalid.
    #[display("Invalid function signature for imported function with index `{_0}`")]
    InvalidImportFnSignature(u32),
    /// Unexpected import kind.
    #[display("Unexpected import kind `{kind}` with index `{index}`")]
    UnexpectedImportKind {
        /// Kind of the import.
        kind: &'static &'static str,
        /// Index of the import.
        index: u32,
    },
}

/// Describes why the code is not valid Gear program.
#[derive(Debug, derive_more::Display, derive_more::From)]
pub enum CodeError {
    /// Validation by wasmparser failed.
    #[display("wasmparser validation error: {_0}")]
    Validation(BinaryReaderError),
    /// Module encode/decode error.
    #[display("Codec error: {_0}")]
    Module(ModuleError),
    /// The provided code contains section error.
    #[display("Section error: {_0}")]
    Section(SectionError),
    /// The provided code contains memory error.
    #[display("Memory error: {_0}")]
    Memory(MemoryError),
    /// The provided code contains stack end error.
    #[display("Stack end error: {_0}")]
    StackEnd(StackEndError),
    /// The provided code contains data section error.
    #[display("Data section error: {_0}")]
    DataSection(DataSectionError),
    /// The provided code contains type section error.
    #[display("Type section error: {_0}")]
    TypeSection(TypeSectionError),
    /// The provided code contains export error.
    #[display("Export error: {_0}")]
    Export(ExportError),
    /// The provided code contains import error.
    #[display("Import error: {_0}")]
    Import(ImportError),
    /// Error occurred during instrumentation WASM module.
    #[display("Instrumentation error: {_0}")]
    Instrumentation(InstrumentationError),
}

impl PartialEq for CodeError {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            // Can't compare BinaryReaderError
            (CodeError::Validation(_), CodeError::Validation(_)) => false,
            // Can't compare ModuleError
            (CodeError::Module(_), CodeError::Module(_)) => false,
            (CodeError::Section(a), CodeError::Section(b)) => a == b,
            (CodeError::Memory(a), CodeError::Memory(b)) => a == b,
            (CodeError::StackEnd(a), CodeError::StackEnd(b)) => a == b,
            (CodeError::DataSection(a), CodeError::DataSection(b)) => a == b,
            (CodeError::TypeSection(a), CodeError::TypeSection(b)) => a == b,
            (CodeError::Export(a), CodeError::Export(b)) => a == b,
            (CodeError::Import(a), CodeError::Import(b)) => a == b,
            // Can't compare InstrumentationError
            (CodeError::Instrumentation(_), CodeError::Instrumentation(_)) => false,
            // Different variants
            _ => false,
        }
    }
}

impl Eq for CodeError {}

impl core::error::Error for CodeError {}
