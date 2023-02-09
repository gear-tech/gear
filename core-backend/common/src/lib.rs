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

pub mod lazy_pages;

mod utils;

#[cfg(feature = "mock")]
pub mod mock;

pub mod memory;

use crate::{memory::MemoryAccessError, utils::TrimmedString};
use alloc::{
    collections::{BTreeMap, BTreeSet},
    string::FromUtf8Error,
    vec::Vec,
};
use codec::{Decode, Encode};
use core::{
    convert::Infallible,
    fmt::{Debug, Display},
};
use gear_core::{
    buffer::RuntimeBufferSizeError,
    env::Ext as EnvExt,
    gas::GasAmount,
    ids::{CodeId, MessageId, ProgramId, ReservationId},
    memory::{GearPage, Memory, MemoryInterval, PageBuf, WasmPage},
    message::{
        ContextStore, Dispatch, DispatchKind, IncomingDispatch, MessageWaitedType,
        PayloadSizeError, WasmEntry,
    },
    reservation::GasReserver,
};
use gear_core_errors::{CoreError, ExecutionError, ExtError, MemoryError, MessageError};
use lazy_pages::GlobalsConfig;
use memory::OutOfMemoryAccessError;
use scale_info::TypeInfo;

// '__gear_stack_end' export is inserted by wasm-proc or wasm-builder
pub const STACK_END_EXPORT_NAME: &str = "__gear_stack_end";

#[derive(Decode, Encode, Debug, PartialEq, Eq, PartialOrd, Ord, Clone, derive_more::From)]
pub enum TerminationReason {
    Exit(ProgramId),
    Leave,
    Success,
    Wait(Option<u32>, MessageWaitedType),
    GasAllowanceExceeded,
    #[from]
    Trap(TrapExplanation),
}

#[derive(
    Decode,
    Encode,
    TypeInfo,
    Debug,
    Clone,
    PartialEq,
    Eq,
    PartialOrd,
    Ord,
    derive_more::Display,
    derive_more::From,
)]
pub enum TrapExplanation {
    #[display(fmt = "{_0}")]
    Core(ExtError),
    #[display(fmt = "{_0}")]
    Panic(TrimmedString),
    #[display(fmt = "Reason is unknown. Possibly `unreachable` instruction is occurred")]
    Unknown,
}

#[derive(Debug, Default)]
pub struct SystemReservationContext {
    /// Reservation created in current execution.
    pub current_reservation: Option<u64>,
    /// Reservation from `ContextStore`.
    pub previous_reservation: Option<u64>,
}

impl SystemReservationContext {
    pub fn from_dispatch(dispatch: &IncomingDispatch) -> Self {
        Self {
            current_reservation: None,
            previous_reservation: dispatch
                .context()
                .as_ref()
                .and_then(|ctx| ctx.system_reservation()),
        }
    }

    pub fn has_any(&self) -> bool {
        self.current_reservation.is_some() || self.previous_reservation.is_some()
    }
}

#[derive(Debug)]
pub struct ExtInfo {
    pub gas_amount: GasAmount,
    pub gas_reserver: GasReserver,
    pub system_reservation_context: SystemReservationContext,
    pub allocations: BTreeSet<WasmPage>,
    pub pages_data: BTreeMap<GearPage, PageBuf>,
    pub generated_dispatches: Vec<(Dispatch, u32, Option<ReservationId>)>,
    pub awakening: Vec<(MessageId, u32)>,
    pub program_candidates_data: BTreeMap<CodeId, Vec<(MessageId, ProgramId)>>,
    pub context_store: ContextStore,
}

pub trait BackendExt: EnvExt {
    fn into_ext_info(self, memory: &impl Memory) -> Result<ExtInfo, MemoryError>;

    fn gas_amount(&self) -> GasAmount;

    /// Pre-process memory access if need.
    fn pre_process_memory_accesses(
        reads: &[MemoryInterval],
        writes: &[MemoryInterval],
    ) -> Result<(), OutOfMemoryAccessError>;
}

pub trait BackendExtError: CoreError + Clone {
    fn from_ext_error(err: ExtError) -> Self;

    fn forbidden_function() -> Self;

    fn into_ext_error(self) -> Result<ExtError, Self>;

    fn into_termination_reason(self) -> TerminationReason;
}

pub struct BackendReport<MemWrap, Ext>
where
    Ext: EnvExt,
{
    pub termination_reason: TerminationReason,
    pub memory_wrap: MemWrap,
    pub ext: Ext,
}

#[derive(Debug, derive_more::Display)]
pub enum EnvironmentExecutionError<Env: Display, PrepMem: Display> {
    #[display(fmt = "Environment error: {_0}")]
    Environment(Env),
    #[display(fmt = "Prepare error: {_1}")]
    PrepareMemory(GasAmount, PrepMem),
    #[display(fmt = "Module start error")]
    ModuleStart(GasAmount),
}

impl<Env: Display, PrepMem: Display> EnvironmentExecutionError<Env, PrepMem> {
    pub fn from_infallible(err: EnvironmentExecutionError<Env, Infallible>) -> Self {
        match err {
            EnvironmentExecutionError::Environment(err) => Self::Environment(err),
            EnvironmentExecutionError::PrepareMemory(_, err) => match err {},
            EnvironmentExecutionError::ModuleStart(gas_amount) => Self::ModuleStart(gas_amount),
        }
    }
}

type EnvironmentBackendReport<Env, EP> =
    BackendReport<<Env as Environment<EP>>::Memory, <Env as Environment<EP>>::Ext>;
pub type EnvironmentExecutionResult<T, Env, EP> = Result<
    EnvironmentBackendReport<Env, EP>,
    EnvironmentExecutionError<<Env as Environment<EP>>::Error, T>,
>;

pub trait Environment<EP = DispatchKind>: Sized
where
    EP: WasmEntry,
{
    type Ext: BackendExt + 'static;

    /// Memory type for current environment.
    type Memory: Memory;

    /// An error issues in environment.
    type Error: Debug + Display;

    /// 1) Instantiates wasm binary.
    /// 2) Creates wasm memory
    /// 3) Runs `pre_execution_handler` to fill the memory before running instance.
    /// 4) Instantiate external funcs for wasm module.
    fn new(
        ext: Self::Ext,
        binary: &[u8],
        entry_point: EP,
        entries: BTreeSet<DispatchKind>,
        mem_size: WasmPage,
    ) -> Result<Self, EnvironmentExecutionError<Self::Error, Infallible>>;

    /// Run instance setup starting at `entry_point` - wasm export function name.
    fn execute<F, T>(self, pre_execution_handler: F) -> EnvironmentExecutionResult<T, Self, EP>
    where
        F: FnOnce(&mut Self::Memory, Option<u32>, GlobalsConfig) -> Result<(), T>,
        T: Display;
}

#[derive(Debug, Clone, derive_more::From)]
pub enum FuncError<E: BackendExtError> {
    Core(E),
    Terminated(TerminationReason),
}

impl<E: BackendExtError> From<MemoryAccessError> for FuncError<E> {
    fn from(err: MemoryAccessError) -> Self {
        match err {
            MemoryAccessError::Memory(err) => E::from_ext_error(err.into()),
            MemoryAccessError::RuntimeBuffer(_) => {
                E::from_ext_error(MemoryError::RuntimeAllocOutOfBounds.into())
            }
            MemoryAccessError::Decode => E::from_ext_error(ExtError::Decode),
        }
        .into()
    }
}

impl<E: BackendExtError> From<FuncError<E>> for TerminationReason {
    fn from(err: FuncError<E>) -> Self {
        match err {
            FuncError::Core(err) => err.into_termination_reason(),
            FuncError::Terminated(reason) => reason,
        }
    }
}

impl<E: BackendExtError> From<PayloadSizeError> for FuncError<E> {
    fn from(_: PayloadSizeError) -> Self {
        E::from_ext_error(MessageError::MaxMessageSizeExceed.into()).into()
    }
}

impl<E: BackendExtError> From<RuntimeBufferSizeError> for FuncError<E> {
    fn from(_: RuntimeBufferSizeError) -> Self {
        E::from_ext_error(MemoryError::RuntimeAllocOutOfBounds.into()).into()
    }
}

impl<E: Display + BackendExtError> From<FromUtf8Error> for FuncError<E> {
    fn from(_err: FromUtf8Error) -> Self {
        E::from_ext_error(ExecutionError::InvalidDebugString.into()).into()
    }
}

pub trait BackendState {
    /// Set termination reason
    fn set_termination_reason(&mut self, reason: TerminationReason);

    /// Set fallible syscall error
    fn set_fallible_syscall_error(&mut self, err: ExtError);

    /// Process fallible syscall function result
    fn process_fallible_func_result<Err: BackendExtError, T: Sized>(
        &mut self,
        res: Result<T, FuncError<Err>>,
    ) -> Result<Result<T, u32>, FuncError<Err>> {
        match res {
            Err(err) => {
                if let FuncError::Core(err) = err {
                    match err.into_ext_error() {
                        Ok(ext_err) => {
                            let len = ext_err.encoded_size() as u32;
                            self.set_fallible_syscall_error(ext_err);
                            Ok(Err(len))
                        }
                        Err(err) => Err(FuncError::Core(err)),
                    }
                } else {
                    Err(err)
                }
            }
            Ok(res) => Ok(Ok(res)),
        }
    }
}

pub trait BackendTermination<E: EnvExt, M: Sized>: Sized {
    /// Into parts
    fn into_parts(self) -> (E, M, TerminationReason);

    /// Terminate backend work after execution
    fn terminate<T: Debug, Err: Debug>(
        self,
        res: Result<T, Err>,
        gas: i64,
        allowance: i64,
    ) -> (E, M, TerminationReason) {
        log::trace!("Execution result = {res:?}");

        let (mut ext, memory, termination_reason) = self.into_parts();

        ext.update_counters(gas as u64, allowance as u64);

        let termination_reason = if res.is_err() {
            if matches!(termination_reason, TerminationReason::Success) {
                // TODO: Parse result error to identify termination reason
                // in case termination occurred inside wasm code.
                TerminationReason::Trap(TrapExplanation::Unknown)
            } else {
                termination_reason
            }
        } else if matches!(termination_reason, TerminationReason::Success) {
            termination_reason
        } else {
            unreachable!(
                "Termination reason is not success, but executor successfully ends execution"
            )
        };

        (ext, memory, termination_reason)
    }
}
