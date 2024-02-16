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

use gear_wasm_instrument::InstrumentationError;

/// Section name in WASM module.
#[derive(Debug, PartialEq, Eq, derive_more::Display)]
pub enum SectionName {
    /// Type section.
    #[display(fmt = "Type section")]
    Type,
    /// Import section.
    #[display(fmt = "Import section")]
    Import,
    /// Function section.
    #[display(fmt = "Function section")]
    Function,
    /// Export section.
    #[display(fmt = "Export section")]
    Export,
    /// Start section.
    #[display(fmt = "Start section")]
    Start,
}

/// Section error in WASM module.
#[derive(Debug, PartialEq, Eq, derive_more::Display)]
pub enum SectionError {
    /// Section not found.
    #[display(fmt = "{_0} not found")]
    NotFound(SectionName),
    /// Section not supported.
    #[display(fmt = "{_0} not supported")]
    NotSupported(SectionName),
}

/// Memory error in WASM module.
#[derive(Debug, PartialEq, Eq, derive_more::Display)]
pub enum MemoryError {
    /// Memory entry not found in import section.
    #[display(fmt = "Memory entry not found")]
    EntryNotFound,
    /// The WASM module has invalid count of static memory pages.
    #[display(fmt = "The WASM module has invalid count of static memory pages")]
    InvalidStaticPageCount,
}

/// Stack end error in WASM module.
#[derive(Debug, PartialEq, Eq, derive_more::Display)]
pub enum StackEndError {
    /// Can't insert new global due to index overflow in global section.
    #[display(fmt = "Can't insert new global due to index overflow")]
    GlobalIndexOverflow,
    /// Pointer to the stack end overlaps data segment.
    #[display(fmt = "Pointer to the stack end overlaps data segment")]
    StackEndOverlaps,
}

/// Initialization error in WASM module.
#[derive(Debug, PartialEq, Eq, derive_more::Display)]
pub enum InitializationError {
    /// Unsupported initialization of gear stack end global variable.
    #[display(fmt = "Unsupported initialization of gear stack end global variable")]
    StackEnd,
    /// Unsupported initialization of data segment.
    #[display(fmt = "Unsupported initialization of data segment")]
    DataSegment,
}

/// Export error in WASM module.
#[derive(Debug, PartialEq, Eq, derive_more::Display)]
pub enum ExportError {
    /// Incorrect global export index. Can occur when export refers to not existing global index.
    #[display(fmt = "Global index `{_0}` in export index `{_1}` is incorrect")]
    IncorrectGlobalIndex(u32, u32),
    /// Exporting mutable globals is restricted by the Gear protocol.
    #[display(fmt = "Global index `{_0}` in export index `{_1}` cannot be mutable")]
    MutableGlobalExport(u32, u32),
    /// Export references to an import function, which is not allowed.
    #[display(fmt = "Export index `{_0}` references to imported function with index `{_1}`")]
    ExportReferencesToImport(u32, u32),
    /// The signature of an exported function is invalid.
    #[display(fmt = "Exported function with index `{_0}` must have signature `fn f() {{ ... }}`")]
    InvalidExportFnSignature(u32),
    /// The provided code contains excess function export.
    #[display(fmt = "Excess export with index `{_0}` found")]
    ExcessExport(u32),
    /// The provided code doesn't contain the required `init` or `handle` export function.
    #[display(fmt = "Required export function `init` or `handle` not found")]
    RequiredExportNotFound,
}

/// Import error in WASM module.
#[derive(Debug, PartialEq, Eq, derive_more::Display)]
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
}

/// Describes why the code is not valid Gear program.
#[derive(Debug, PartialEq, Eq, derive_more::Display, derive_more::From)]
pub enum CodeError {
    /// Validation by wasmparser failed.
    #[display(fmt = "Wasm validation failed")]
    Validation,
    /// Error occurred during decoding original program code.
    #[display(fmt = "The wasm bytecode is failed to be decoded")]
    Decode,
    /// Error occurred during encoding instrumented program.
    #[display(fmt = "Failed to encode instrumented program")]
    Encode,
    /// The provided code contains section error.
    #[display(fmt = "Section error: {_0}")]
    Section(SectionError),
    /// The provided code contains memory error.
    #[display(fmt = "Memory error: {_0}")]
    Memory(MemoryError),
    /// The provided code contains stack end error.
    #[display(fmt = "Stack end error: {_0}")]
    StackEnd(StackEndError),
    /// The provided code contains initialization error.
    #[display(fmt = "Initialization error: {_0}")]
    Initialization(InitializationError),
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
