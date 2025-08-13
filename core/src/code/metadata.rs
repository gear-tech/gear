// This file is part of Gear.

// Copyright (C) 2021-2025 Gear Technologies Inc.
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

//! Module code metadata

use crate::{
    message::DispatchKind,
    pages::{WasmPage, WasmPagesAmount},
};
use alloc::collections::BTreeSet;
use scale_info::{
    TypeInfo,
    scale::{Decode, Encode},
};

/// Status of the instrumentation.
#[derive(Clone, Copy, Debug, Decode, Encode, TypeInfo, PartialEq, Eq, Hash)]
pub enum InstrumentationStatus {
    /// Code is not instrumented yet.
    NotInstrumented,
    /// Code is instrumented on weights version.
    Instrumented {
        /// Version of the instruction weights used for instrumentation.
        version: u32,
        /// Instrumented code length.
        code_len: u32,
    },
    /// Failed to instrument code on weights version.
    InstrumentationFailed {
        /// Version of the instruction weights used for instrumentation.
        version: u32,
    },
}

/// Metadata for the code.
#[derive(Clone, Debug, Decode, Encode, TypeInfo, PartialEq, Eq, Hash)]
pub struct CodeMetadata {
    /// Original code length.
    original_code_len: u32,
    /// Exports of the wasm module.
    exports: BTreeSet<DispatchKind>,
    // Static pages count from memory import.
    static_pages: WasmPagesAmount,
    /// Stack end page.
    stack_end: Option<WasmPage>,
    /// Instrumentation status, contains version of the instructions in case of instrumentation.
    instrumentation_status: InstrumentationStatus,
}

impl CodeMetadata {
    /// Creates a new instance of the code metadata.
    pub fn new(
        original_code_len: u32,
        exports: BTreeSet<DispatchKind>,
        static_pages: WasmPagesAmount,
        stack_end: Option<WasmPage>,
        instrumentation_status: InstrumentationStatus,
    ) -> Self {
        Self {
            original_code_len,
            exports,
            static_pages,
            stack_end,
            instrumentation_status,
        }
    }

    /// Converts the metadata into the failed instrumentation state.
    pub fn into_failed_instrumentation(self, instruction_weights_version: u32) -> Self {
        Self {
            instrumentation_status: InstrumentationStatus::InstrumentationFailed {
                version: instruction_weights_version,
            },
            ..self
        }
    }

    /// Returns the original code length.
    pub fn original_code_len(&self) -> u32 {
        self.original_code_len
    }

    /// Returns the instrumented code length.
    pub fn instrumented_code_len(&self) -> Option<u32> {
        match self.instrumentation_status {
            InstrumentationStatus::NotInstrumented
            | InstrumentationStatus::InstrumentationFailed { .. } => None,
            InstrumentationStatus::Instrumented { code_len, .. } => Some(code_len),
        }
    }

    /// Returns the code exports.
    pub fn exports(&self) -> &BTreeSet<DispatchKind> {
        &self.exports
    }

    /// Returns the static pages count from memory import.
    pub fn static_pages(&self) -> WasmPagesAmount {
        self.static_pages
    }

    /// Returns the stack end page.
    pub fn stack_end(&self) -> Option<WasmPage> {
        self.stack_end
    }

    /// Returns the instrumentation status.
    pub fn instrumentation_status(&self) -> InstrumentationStatus {
        self.instrumentation_status
    }

    /// Returns the version of the instructions.
    pub fn instruction_weights_version(&self) -> Option<u32> {
        match self.instrumentation_status {
            InstrumentationStatus::NotInstrumented => None,
            InstrumentationStatus::Instrumented { version, .. } => Some(version),
            InstrumentationStatus::InstrumentationFailed { version } => Some(version),
        }
    }
}
