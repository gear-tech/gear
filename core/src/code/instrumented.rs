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

//! Module for instrumented code.

use crate::{
    code::CodeAndId,
    ids::CodeId,
    message::DispatchKind,
    pages::{WasmPage, WasmPagesAmount},
};
use alloc::{collections::BTreeSet, vec::Vec};
use scale_info::{
    scale::{Decode, Encode},
    TypeInfo,
};

/// The newtype contains the instrumented code and the corresponding id (hash).
#[derive(Clone, Debug, Decode, Encode, TypeInfo)]
pub struct InstrumentedCode {
    pub(crate) code: Vec<u8>,
    pub(crate) original_code_len: u32,
    pub(crate) exports: BTreeSet<DispatchKind>,
    pub(crate) static_pages: WasmPagesAmount,
    pub(crate) stack_end: Option<WasmPage>,
    pub(crate) version: u32,
}

impl InstrumentedCode {
    /// Creates a new instance of the instrumented code.
    ///
    /// # Safety
    /// The caller must ensure that the `code` is a valid wasm binary,
    /// and other parameters are valid and suitable for the wasm binary.
    pub unsafe fn new_unchecked(
        code: Vec<u8>,
        original_code_len: u32,
        exports: BTreeSet<DispatchKind>,
        static_pages: WasmPagesAmount,
        stack_end: Option<WasmPage>,
        version: u32,
    ) -> Self {
        Self {
            code,
            original_code_len,
            exports,
            static_pages,
            stack_end,
            version,
        }
    }

    /// Returns reference to the instrumented binary code.
    pub fn code(&self) -> &[u8] {
        &self.code
    }

    /// Returns the length of the original binary code.
    pub fn original_code_len(&self) -> u32 {
        self.original_code_len
    }

    /// Returns instruction weights version.
    pub fn instruction_weights_version(&self) -> u32 {
        self.version
    }

    /// Returns wasm module exports.
    pub fn exports(&self) -> &BTreeSet<DispatchKind> {
        &self.exports
    }

    /// Returns initial memory size from memory import.
    pub fn static_pages(&self) -> WasmPagesAmount {
        self.static_pages
    }

    /// Returns stack end page if exists.
    pub fn stack_end(&self) -> Option<WasmPage> {
        self.stack_end
    }

    /// Consumes the instance and returns the instrumented code.
    pub fn into_code(self) -> Vec<u8> {
        self.code
    }
}

/// The newtype contains the instrumented code and the corresponding id (hash).
#[derive(Clone, Debug)]
pub struct InstrumentedCodeAndId {
    code: InstrumentedCode,
    code_id: CodeId,
}

impl InstrumentedCodeAndId {
    /// Returns reference to the instrumented code.
    pub fn code(&self) -> &InstrumentedCode {
        &self.code
    }

    /// Returns corresponding id (hash) for the code.
    pub fn code_id(&self) -> CodeId {
        self.code_id
    }

    /// Consumes the instance and returns the instrumented code.
    pub fn into_parts(self) -> (InstrumentedCode, CodeId) {
        (self.code, self.code_id)
    }
}

impl From<CodeAndId> for InstrumentedCodeAndId {
    fn from(code_and_id: CodeAndId) -> Self {
        let (code, code_id) = code_and_id.into_parts();
        let (code, _) = code.into_parts();
        Self { code, code_id }
    }
}
