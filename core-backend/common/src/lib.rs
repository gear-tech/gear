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

use crate::{
    memory::{ActorMemoryAccessError, MemoryAccessError, SystemMemoryAccessError},
    utils::TrimmedString,
};
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
    env::Ext,
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

// '__gear_stack_end' export is inserted in wasm-proc or wasm-builder
pub const STACK_END_EXPORT_NAME: &str = "__gear_stack_end";

#[derive(Decode, Encode, Debug, PartialEq, Eq, PartialOrd, Ord, Clone, derive_more::From)]
pub enum TerminationReason {
    Exit(ProgramId),
    Leave,
    Success,
    #[from]
    Trap(TrapExplanation),
    Wait(Option<u32>, MessageWaitedType),
    GasAllowanceExceeded,
}

#[derive(
    Decode, Encode, TypeInfo, Debug, Clone, PartialEq, Eq, PartialOrd, Ord, derive_more::Display,
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

pub trait BackendExt: Ext {
    fn into_ext_info(self, memory: &impl Memory) -> Result<ExtInfo, MemoryError>;

    fn into_gas_amount(self) -> GasAmount;

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

pub trait BackendState<E: BackendExtError> {
    fn err_mut(&mut self) -> &mut SyscallFuncError<E>;

    fn last_err(&mut self) -> Result<ExtError, ExtError> {
        let last_err = match self.err_mut().clone() {
            SyscallFuncError::Actor(ActorSyscallFuncError::Core(maybe_ext)) => maybe_ext
                .into_ext_error()
                .map_err(|_| ExtError::SyscallUsage),
            _ => Err(ExtError::SyscallUsage),
        };

        if let Err(err) = &last_err {
            *self.err_mut() = ActorSyscallFuncError::Core(E::from_ext_error(err.clone())).into();
        }

        last_err
    }
}

pub trait IntoExtErrorForResult<T, Err, State>
where
    Err: Display,
{
    fn into_ext_error(
        self,
        state: &mut State,
    ) -> Result<Result<T, u32>, ActorSyscallFuncError<Err>>;
}

impl<T, Err, State> IntoExtErrorForResult<T, Err, State> for Result<T, Err>
where
    Err: BackendExtError + Display,
    State: BackendState<Err>,
{
    fn into_ext_error(
        self,
        state: &mut State,
    ) -> Result<Result<T, u32>, ActorSyscallFuncError<Err>> {
        match self {
            Ok(value) => Ok(Ok(value)),
            Err(err) => {
                *state.err_mut() = ActorSyscallFuncError::Core(err.clone()).into();
                match err.into_ext_error() {
                    Ok(ext_err) => Ok(Err(ext_err.encoded_size() as u32)),
                    Err(err) => Err(ActorSyscallFuncError::Core(err)),
                }
            }
        }
    }
}

pub trait GetGasAmount {
    fn gas_amount(&self) -> GasAmount;
}

pub struct BackendReport<T, E> {
    pub termination_reason: TerminationReason,
    pub memory_wrap: T,
    pub ext: E,
}

#[derive(Debug)]
pub enum EnvironmentExecutionError<Env, PrepMem> {
    Environment(Env),
    PrepareMemory(GasAmount, PrepMem),
    ModuleStart(GasAmount),
    SyscallFunc(SystemSyscallFuncError),
}

impl<Env, PrepMem> EnvironmentExecutionError<Env, PrepMem> {
    pub fn from_infallible(err: EnvironmentExecutionError<Env, Infallible>) -> Self {
        match err {
            EnvironmentExecutionError::Environment(err) => Self::Environment(err),
            EnvironmentExecutionError::PrepareMemory(_, err) => match err {},
            EnvironmentExecutionError::ModuleStart(gas_amount) => Self::ModuleStart(gas_amount),
            EnvironmentExecutionError::SyscallFunc(err) => Self::SyscallFunc(err),
        }
    }
}

pub trait Environment<E, EP = DispatchKind>: Sized
where
    E: BackendExt + 'static,
    EP: WasmEntry,
{
    /// Memory type for current environment.
    type Memory: Memory;

    /// An error issues in environment.
    type Error: Debug + Display + GetGasAmount;

    /// 1) Instantiates wasm binary.
    /// 2) Creates wasm memory
    /// 3) Runs `pre_execution_handler` to fill the memory before running instance.
    /// 4) Instantiate external funcs for wasm module.
    fn new(
        ext: E,
        binary: &[u8],
        entry_point: EP,
        entries: BTreeSet<DispatchKind>,
        mem_size: WasmPage,
    ) -> Result<Self, EnvironmentExecutionError<Self::Error, Infallible>>;

    /// Run instance setup starting at `entry_point` - wasm export function name.
    fn execute<F, T>(
        self,
        pre_execution_handler: F,
    ) -> Result<BackendReport<Self::Memory, E>, EnvironmentExecutionError<Self::Error, T>>
    where
        F: FnOnce(&mut Self::Memory, Option<i32>, GlobalsConfig) -> Result<(), T>;
}

#[derive(Debug, Clone, derive_more::From)]
pub enum SyscallFuncError<E: Display> {
    Actor(ActorSyscallFuncError<E>),
    System(SystemSyscallFuncError),
}

impl<E: Display + BackendExtError> From<MemoryAccessError> for SyscallFuncError<E> {
    fn from(err: MemoryAccessError) -> Self {
        match err {
            MemoryAccessError::Actor(err) => ActorSyscallFuncError::from(err).into(),
            MemoryAccessError::System(err) => SystemSyscallFuncError::from(err).into(),
        }
    }
}

impl<E: Display + BackendExtError> From<PayloadSizeError> for SyscallFuncError<E> {
    fn from(err: PayloadSizeError) -> Self {
        ActorSyscallFuncError::from(err).into()
    }
}

impl<E: Display + BackendExtError> From<RuntimeBufferSizeError> for SyscallFuncError<E> {
    fn from(err: RuntimeBufferSizeError) -> Self {
        ActorSyscallFuncError::from(err).into()
    }
}

impl<E: Display + BackendExtError> From<FromUtf8Error> for SyscallFuncError<E> {
    fn from(_err: FromUtf8Error) -> Self {
        ActorSyscallFuncError::Core(E::from_ext_error(ExecutionError::InvalidDebugString.into()))
            .into()
    }
}

#[derive(Debug, Clone, derive_more::Display, derive_more::From)]
pub enum ActorSyscallFuncError<E: Display> {
    #[display(fmt = "{_0}")]
    Core(E),
    #[from]
    #[display(fmt = "Terminated: {_0:?}")]
    Terminated(TerminationReason),
}

impl<E: Display + BackendExtError> From<PayloadSizeError> for ActorSyscallFuncError<E> {
    fn from(_err: PayloadSizeError) -> Self {
        Self::Core(E::from_ext_error(MessageError::MaxMessageSizeExceed.into()))
    }
}

impl<E: Display + BackendExtError> From<RuntimeBufferSizeError> for ActorSyscallFuncError<E> {
    fn from(_err: RuntimeBufferSizeError) -> Self {
        Self::Core(E::from_ext_error(ExtError::SyscallUsage))
    }
}

impl<E: Display + BackendExtError> From<ActorMemoryAccessError> for ActorSyscallFuncError<E> {
    fn from(err: ActorMemoryAccessError) -> Self {
        match err {
            ActorMemoryAccessError::Memory(err) => Self::Core(E::from_ext_error(err.into())),
            ActorMemoryAccessError::RuntimeBuffer(err) => Self::from(err),
        }
    }
}

impl<E: Display + BackendExtError> ActorSyscallFuncError<E> {
    pub fn into_termination_reason(self) -> TerminationReason {
        match self {
            Self::Core(err) => err.into_termination_reason(),
            Self::Terminated(reason) => reason,
        }
    }
}

#[derive(Debug, Clone, derive_more::Display, derive_more::From)]
pub enum SystemSyscallFuncError {
    #[display(fmt = "Binary code has wrong instrumentation")]
    WrongInstrumentation,
    #[from]
    #[display(fmt = "Memory access error: {_0}")]
    MemoryAccess(SystemMemoryAccessError),
}
