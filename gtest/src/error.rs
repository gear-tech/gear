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

use gear_core::{ids::ActorId, pages::WasmPage};
use gear_core_errors::ExtError;
use parity_scale_codec::Error as CodecError;

/// Type alias for the testing functions running result.
pub type Result<T, E = TestError> = core::result::Result<T, E>;

/// List of general errors.
#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
pub enum TestError {
    /// Invalid return type after execution.
    #[error("Invalid return type after execution")]
    InvalidReturnType,

    /// Function not found in executor.
    #[error("Function not found in executor: `{0}`")]
    FunctionNotFound(String),

    /// Actor not found.
    #[error("Actor not found: `{0}`")]
    ActorNotFound(ActorId),

    /// Actor is not executable.
    #[error("Actor is not executable: `{0}`")]
    ActorIsNotExecutable(ActorId),

    /// Meta WASM binary hasn't been provided.
    #[error("Meta WASM binary hasn't been provided")]
    MetaBinaryNotProvided,

    /// Insufficient memory.
    #[error("Insufficient memory: available {0:?} < requested {1:?}")]
    InsufficientMemory(WasmPage, WasmPage),

    /// Invalid import module.
    #[error("Invalid import module: `{0}` instead of `env`")]
    InvalidImportModule(String),

    /// Failed to call unsupported function.
    #[error("Failed to call unsupported function: `{0}`")]
    UnsupportedFunction(String),

    /// Wrapper for [`ExtError`].
    #[error(transparent)]
    ExecutionError(#[from] ExtError),

    /// Wrapper for [`wasmi::Error`](https://paritytech.github.io/wasmi/wasmi/enum.Error.html).
    #[error(transparent)]
    MemoryError(#[from] gear_core_errors::MemoryError),

    /// Wrapper for [`parity_scale_codec::Error`](https://docs.rs/parity-scale-codec/latest/parity_scale_codec/struct.Error.html).
    #[error("Codec error: `{0}`")]
    ScaleCodecError(#[from] CodecError),

    /// Instrumentation of binary code failed.
    #[error("Instrumentation of binary code failed.")]
    Instrumentation,

    /// Reading of program state failed.
    #[error("Reading of program state failed: `{0}`")]
    ReadStateError(String),

    /// Searching gbuild artifact failed.
    #[error("Reading of program state failed: `{0}`")]
    GbuildArtifactNotFound(String),
}

macro_rules! usage_panic {
    ($($arg:tt)*) => {{
        use colored::Colorize as _;
        let panic_msg = format!($($arg)*).red().bold();
        panic!("{}", panic_msg);
    }};
}

pub(crate) use usage_panic;
