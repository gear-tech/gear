// This file is part of Gear.

// Copyright (C) 2021-2022 Gear Technologies Inc.
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

//! Crate provides support for wasm runtime.

#![no_std]

extern crate alloc;

pub mod error_processor;
pub mod funcs;

use alloc::{
    borrow::Cow,
    collections::{BTreeMap, BTreeSet},
    string::String,
    vec::Vec,
};
use codec::{Decode, Encode};
use core::fmt;
use gear_core::{
    env::Ext,
    gas::GasAmount,
    ids::{CodeId, MessageId, ProgramId},
    memory::{Memory, PageBuf, PageNumber, WasmPageNumber},
    message::{ContextStore, Dispatch},
};
use gear_core_errors::{ExtError, MemoryError};
use scale_info::TypeInfo;

#[derive(
    Decode, Encode, TypeInfo, Debug, Clone, PartialEq, Eq, PartialOrd, Ord, derive_more::Display,
)]
pub struct TrimmedString(String);

// Amount of characters allowed to be thrown as string explanation of the error.
const TRIMMING_LEN: usize = 1024;

impl<T: Into<String>> From<T> for TrimmedString {
    fn from(other: T) -> Self {
        let mut string = other.into();

        if string.len() >= TRIMMING_LEN {
            string.truncate(TRIMMING_LEN - 4);
            string.push_str(" ...")
        }

        Self(string)
    }
}

#[derive(Decode, Encode, Debug, Clone, Copy, Eq, PartialEq, Hash)]
pub enum TerminationReasonKind {
    Exit,
    Leave,
    Wait,
    GasAllowanceExceeded,
    ForbiddenFunction,
}

#[derive(Decode, Encode, Debug, PartialEq, Eq, PartialOrd, Ord, Clone)]
pub enum TerminationReason {
    Exit(ProgramId),
    Leave,
    Success,
    Trap {
        explanation: Option<TrapExplanation>,
        description: Option<Cow<'static, str>>,
    },
    Wait,
    GasAllowanceExceeded,
}

#[derive(
    Decode, Encode, TypeInfo, Debug, Clone, PartialEq, Eq, PartialOrd, Ord, derive_more::Display,
)]
pub enum TrapExplanation {
    #[display(fmt = "{}", _0)]
    Core(ExtError),
    #[display(fmt = "{}", _0)]
    Other(TrimmedString),
    #[display(fmt = "Unreachable instruction")]
    Unreachable,
}

#[derive(Debug)]
pub struct ExtInfo {
    pub gas_amount: GasAmount,
    pub allocations: BTreeSet<WasmPageNumber>,
    pub pages_data: BTreeMap<PageNumber, PageBuf>,
    pub generated_dispatches: Vec<Dispatch>,
    pub awakening: Vec<MessageId>,
    pub program_candidates_data: BTreeMap<CodeId, Vec<(ProgramId, MessageId)>>,
    pub context_store: ContextStore,
    pub trap_explanation: Option<TrapExplanation>,
    pub exit_argument: Option<ProgramId>,
}

pub trait IntoExtInfo {
    fn into_ext_info(self, memory: &dyn Memory) -> Result<ExtInfo, (MemoryError, GasAmount)>;

    fn into_gas_amount(self) -> GasAmount;

    fn last_error(&self) -> Option<&ExtError>;
}

pub struct BackendReport {
    pub termination: TerminationReason,
    pub info: ExtInfo,
}

#[derive(Debug)]
pub struct BackendError<T> {
    pub gas_amount: GasAmount,
    pub reason: T,
    pub description: Option<Cow<'static, str>>,
}

impl<T> fmt::Display for BackendError<T>
where
    T: fmt::Display,
{
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        if let Some(description) = &self.description {
            write!(f, "{}: {}", self.reason, description)
        } else {
            write!(f, "{}", self.reason)
        }
    }
}

pub trait Environment<E: Ext + IntoExtInfo + 'static>: Sized {
    /// An error issues in environment
    type Error: fmt::Display;

    /// Creates new external environment to execute wasm binary:
    /// 1) Instantiates wasm binary.
    /// 2) Creates wasm memory with filled data (exception if lazy pages enabled).
    /// 3) Instantiate external funcs for wasm module.
    fn new(
        ext: E,
        binary: &[u8],
        mem_size: WasmPageNumber,
    ) -> Result<Self, BackendError<Self::Error>>;

    /// Returns addr to the stack end if it can be identified
    fn get_stack_mem_end(&mut self) -> Option<WasmPageNumber>;

    /// Get ref to mem wrapper
    fn get_mem(&self) -> &dyn Memory;

    /// Get mut ref to mem wrapper
    fn get_mem_mut(&mut self) -> &mut dyn Memory;

    /// Run instance setup starting at `entry_point` - wasm export function name.
    /// Also runs `post_execution_handler` after running instance at provided entry point.
    fn execute<F, T>(
        self,
        entry_point: &str,
        post_execution_handler: F,
    ) -> Result<BackendReport, BackendError<Self::Error>>
    where
        F: FnOnce(&dyn Memory) -> Result<(), T>,
        T: fmt::Display;

    /// Consumes environment and returns gas state.
    fn into_gas_amount(self) -> GasAmount;
}

pub trait AsTerminationReason {
    fn as_termination_reason(&self) -> Option<&TerminationReasonKind>;
}
