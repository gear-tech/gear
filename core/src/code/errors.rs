// This file is part of Gear.

// Copyright (C) 2024 Gear Technologies Inc.
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

pub use gear_wasm_instrument::{parity_wasm::SerializationError, InstrumentationError};
pub use wasmparser::BinaryReaderError;

/// Section name in WASM module.
#[derive(PartialEq, Eq, Debug, derive_more::Display)]
pub enum SectionName {
    /// Type section.
    #[display(fmt = "Type section")]
    Type,
    /// Import section.
    #[display(fmt = "Import section")]
    Import,
    /// Function (Code) section.
    #[display(fmt = "Function section")]
    Function,
    /// Data section.
    #[display(fmt = "Data section")]
    Data,
    /// Global section.
    #[display(fmt = "Global section")]
    Global,
    /// Table section.
    #[display(fmt = "Table section")]
    Table,
    /// Element section.
    #[display(fmt = "Element section")]
    Element,
    /// Export section.
    #[display(fmt = "Export section")]
    Export,
    /// Start section.
    #[display(fmt = "Start section")]
    Start,
}

/// Section error in WASM module.
#[derive(Debug, derive_more::Display)]
pub enum SectionError {
    /// Section not found.
    #[display(fmt = "{_0} not found")]
    NotFound(SectionName),
    /// Section not supported.
    #[display(fmt = "{_0} not supported")]
    NotSupported(SectionName),
}

/// Memory error in WASM module.
#[derive(Debug, derive_more::Display)]
pub enum MemoryError {
    /// Memory entry not found in import section.
    #[display(fmt = "Memory entry not found")]
    EntryNotFound,
    /// The WASM module has invalid count of static memory pages.
    #[display(fmt = "The WASM module has invalid count of static memory pages")]
    InvalidStaticPageCount,
}

/// Stack end error in WASM module.
#[derive(Debug, derive_more::Display)]
pub enum StackEndError {
    /// Unsupported initialization of gear stack end global variable.
    #[display(fmt = "Unsupported initialization of gear stack end global")]
    Initialization,
    /// Gear stack end offset is not aligned to wasm page size.
    #[display(fmt = "Gear stack end {_0:#x} is not aligned to wasm page size")]
    NotAligned(u32),
    /// Gear stack end is out of static memory.
    #[display(fmt = "Gear stack end {_0:#x} is out of static memory 0x0..{_1:#x}")]
    OutOfStatic(u32, u64),
}

/// Data section error in WASM module.
#[derive(Debug, derive_more::Display)]
pub enum DataSectionError {
    /// Unsupported initialization of data segment.
    #[display(fmt = "Unsupported initialization of data segment")]
    Initialization,
    /// Data section overlaps gear stack.
    #[display(fmt = "Data segment {_0:#x} overlaps gear stack 0x0..{_1:#x}")]
    GearStackOverlaps(u32, u32),
    /// Data segment end address is out of possible 32 bits address space.
    #[display(fmt = "Data segment {_0:#x} ends out of possible 32 bits address space")]
    EndAddressOverflow(u32),
    /// Data segment end address is out of static memory.
    #[display(fmt = "Data segment {_0:#x}..={_1:#x} is out of static memory 0x0..{_2:#x}")]
    EndAddressOutOfStaticMemory(u32, u32, u64),
    /// Data segment amount exceeds the limit.
    #[display(fmt = "Data segment amount limit exceeded: limit={limit}, actual={actual}")]
    DataSegmentsAmountLimit {
        /// Limit of data segments.
        limit: u32,
        /// Actual amount of data segments.
        actual: u32,
    },
}

/// Table section error in WASM module.
#[derive(Debug, derive_more::Display)]
pub enum TableSectionError {
    /// Number of table exceeds the limit.
    #[display(fmt = "Number of table limit exceeded: limit={limit}, actual={actual}")]
    TableNumberLimit {
        /// Limit on the number of tables.
        limit: u32,
        /// Actual number of tables.
        actual: u32,
    },
}

/// Export error in WASM module.
#[derive(Debug, derive_more::Display)]
pub enum ExportError {
    /// Incorrect global export index. Can occur when export refers to not existing global index.
    #[display(fmt = "Global index `{_0}` in export index `{_1}` is incorrect")]
    IncorrectGlobalIndex(u32, u32),
    /// Exporting mutable globals is restricted by the Gear protocol.
    #[display(fmt = "Global index `{_0}` in export index `{_1}` cannot be mutable")]
    MutableGlobalExport(u32, u32),
    /// Export references to an import function, which is not allowed.
    #[display(fmt = "Export index `{_0}` references to imported function with index `{_1}`")]
    ExportReferencesToImportFunction(u32, u32),
    /// Export references to an import global, which is not allowed.
    #[display(fmt = "Export index `{_0}` references to imported global with index `{_1}`")]
    ExportReferencesToImportGlobal(u32, u32),
    /// The signature of an exported function is invalid.
    #[display(fmt = "Exported function with index `{_0}` must have signature `fn f() {{ ... }}`")]
    InvalidExportFnSignature(u32),
    /// The provided code contains excess function export.
    #[display(fmt = "Excess export with index `{_0}` found")]
    ExcessExport(u32),
    /// The provided code doesn't contain the required `init` or `handle` export function.
    #[display(fmt = "Required export function `init` or `handle` is not found")]
    RequiredExportNotFound,
}

/// Import error in WASM module.
#[derive(Debug, derive_more::Display)]
pub enum ImportError {
    /// The imported function is not supported by the Gear protocol.
    #[display(fmt = "Unknown imported function with index `{_0}`")]
    UnknownImport(u32),
    /// The imported function is declared multiple times.
    #[display(fmt = "Imported function with index `{_0}` is declared multiple times")]
    DuplicateImport(u32),
    /// The signature of an imported function is invalid.
    #[display(fmt = "Invalid function signature for imported function with index `{_0}`")]
    InvalidImportFnSignature(u32),
    /// Unexpected import kind.
    #[display(fmt = "Unexpected import kind `{kind}` with index `{index}`")]
    UnexpectedImportKind {
        /// Kind of the import.
        kind: &'static &'static str,
        /// Index of the import.
        index: u32,
    },
}

/// Module encode/decode error.
#[derive(Debug, derive_more::Display)]
pub enum CodecError {
    /// The wasm bytecode is failed to be decoded
    #[display(fmt = "The wasm bytecode is failed to be decoded: {_0}")]
    Decode(BinaryReaderError),
    /// Failed to encode instrumented program
    #[display(fmt = "Failed to encode instrumented program: {_0}")]
    Encode(SerializationError),
}

/// Describes why the code is not valid Gear program.
#[derive(Debug, derive_more::Display, derive_more::From)]
pub enum CodeError {
    /// Validation by wasmparser failed.
    #[display(fmt = "Wasmer validation error: {_0}")]
    Validation(BinaryReaderError),
    /// Module encode/decode error.
    #[display(fmt = "Codec error: {_0}")]
    Codec(CodecError),
    /// The provided code contains section error.
    #[display(fmt = "Section error: {_0}")]
    Section(SectionError),
    /// The provided code contains memory error.
    #[display(fmt = "Memory error: {_0}")]
    Memory(MemoryError),
    /// The provided code contains stack end error.
    #[display(fmt = "Stack end error: {_0}")]
    StackEnd(StackEndError),
    /// The provided code contains data section error.
    #[display(fmt = "Data section error: {_0}")]
    DataSection(DataSectionError),
    /// The provided code contains table section error.
    #[display(fmt = "Table section error: {_0}")]
    TableSection(TableSectionError),
    /// The provided code contains export error.
    #[display(fmt = "Export error: {_0}")]
    Export(ExportError),
    /// The provided code contains import error.
    #[display(fmt = "Import error: {_0}")]
    Import(ImportError),
    /// Error occurred during instrumentation WASM module.
    #[display(fmt = "Instrumentation error: {_0}")]
    Instrumentation(InstrumentationError),
}
