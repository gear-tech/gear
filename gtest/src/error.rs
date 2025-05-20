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
#[derive(Debug, derive_more::Display, derive_more::From)]
pub enum TestError {
    /// Invalid return type after execution.
    #[display("Invalid return type after execution")]
    InvalidReturnType,

    /// Function not found in executor.
    #[from(ignore)]
    #[display("Function not found in executor: `{_0}`")]
    FunctionNotFound(String),

    /// Actor not found.
    #[from(ignore)]
    #[display("Actor not found: `{_0}`")]
    ActorNotFound(ActorId),

    /// Actor is not executable.
    #[from(ignore)]
    #[display("Actor is not executable: `{_0}`")]
    ActorIsNotExecutable(ActorId),

    /// Meta WASM binary hasn't been provided.
    #[display("Meta WASM binary hasn't been provided")]
    MetaBinaryNotProvided,

    /// Insufficient memory.
    #[display("Insufficient memory: available {_0:?} < requested {_1:?}")]
    InsufficientMemory(WasmPage, WasmPage),

    /// Invalid import module.
    #[from(ignore)]
    #[display("Invalid import module: `{_0}` instead of `env`")]
    InvalidImportModule(String),

    /// Failed to call unsupported function.
    #[from(ignore)]
    #[display("Failed to call unsupported function: `{_0}`")]
    UnsupportedFunction(String),

    /// Wrapper for [`ExtError`].
    ExecutionError(ExtError),

    /// Wrapper for [`wasmi::Error`](https://paritytech.github.io/wasmi/wasmi/enum.Error.html).
    MemoryError(gear_core_errors::MemoryError),

    /// Wrapper for [`parity_scale_codec::Error`](https://docs.rs/parity-scale-codec/latest/parity_scale_codec/struct.Error.html).
    ScaleCodecError(CodecError),

    /// Instrumentation of binary code failed.
    #[display("Instrumentation of binary code failed.")]
    Instrumentation,

    /// Reading of program state failed.
    #[display("Reading of program state failed: `{_0}`")]
    ReadStateError(String),

    /// Searching gbuild artifact failed.
    #[from(ignore)]
    #[display("Reading of program state failed: `{_0}`")]
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
