// This file is part of Gear.

// Copyright (C) 2022 Gear Technologies Inc.
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

use anyhow::Error as AnyhowError;
use codec::Error as CodecError;
use core_processor::ProcessorError;
use gear_core::{ids::ProgramId, memory::WasmPageNumber};
use wasmtime::MemoryAccessError;

/// Type alias for the testing functions running result.
pub type Result<T, E = TestError> = core::result::Result<T, E>;

/// List of general errors.
#[derive(Debug, derive_more::Display)]
pub enum TestError {
    /// Invalid return type after execution.
    #[display(fmt = "Invalid return type after execution")]
    InvalidReturnType,

    /// Function not found in executor.
    #[display(fmt = "Function not found in executor: `{}`", _0)]
    FunctionNotFound(String),

    /// Actor not found.
    #[display(fmt = "Actor not found: `{:?}`", _0)]
    ActorNotFound(ProgramId),

    /// Actor is not executable.
    #[display(fmt = "Actor is not executable: `{:?}`", _0)]
    ActorIsntExecutable(ProgramId),

    /// Meta WASM binary hasn't been provided.
    #[display(fmt = "Meta WASM binary hasn't been provided")]
    MetaBinaryNotProvided,

    /// Insufficient memory.
    #[display(fmt = "Insufficient memory: available {:?} < requested {:?}", _0, _1)]
    InsufficientMemory(WasmPageNumber, WasmPageNumber),

    /// Invalid import module.
    #[display(fmt = "Invalid import module: `{}` instead of `env`", _0)]
    InvalidImportModule(String),

    /// Failed to call unsupported function.
    #[display(fmt = "Failed to call unsupported function: `{}`", _0)]
    UnsupportedFunction(String),

    /// Wrapper for [`ProcessorError`].
    #[display(fmt = "{}", _0)]
    ExecutionError(ProcessorError),

    /// Wrapper for [`MemoryAccessError`].
    #[display(fmt = "{}", _0)]
    MemoryError(MemoryAccessError),

    /// Wrapper for `wasmtime` error (used [`anyhow::Error`] for that).
    #[display(fmt = "{}", _0)]
    WasmtimeError(AnyhowError),

    /// Wrapper for `parity-scale-codec` error (see [`parity_scale_codec::Error`]).
    #[display(fmt = "{}", _0)]
    ScaleCodecError(CodecError),
}

impl From<ProcessorError> for TestError {
    fn from(err: ProcessorError) -> Self {
        TestError::ExecutionError(err)
    }
}

impl From<MemoryAccessError> for TestError {
    fn from(err: MemoryAccessError) -> Self {
        TestError::MemoryError(err)
    }
}

impl From<AnyhowError> for TestError {
    fn from(err: AnyhowError) -> Self {
        TestError::WasmtimeError(err)
    }
}

impl From<CodecError> for TestError {
    fn from(err: CodecError) -> Self {
        TestError::ScaleCodecError(err)
    }
}
