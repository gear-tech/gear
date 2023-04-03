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

use crate::memory::MemoryAccessError;
use alloc::{
    collections::{BTreeMap, BTreeSet},
    string::{FromUtf8Error, String},
    vec::Vec,
};
use core::{
    convert::Infallible,
    fmt::{Debug, Display},
};
use gear_core::{
    buffer::RuntimeBufferSizeError,
    env::Ext as EnvExt,
    gas::{ChargeError, CountersOwner, GasAmount, GasLeft},
    ids::{CodeId, MessageId, ProgramId, ReservationId},
    memory::{GearPage, IncorrectAllocationDataError, Memory, MemoryInterval, PageBuf, WasmPage},
    message::{
        ContextStore, Dispatch, DispatchKind, IncomingDispatch, MessageWaitedType,
        PayloadSizeError, WasmEntry,
    },
    reservation::GasReserver,
};
use gear_core_errors::{ExecutionError, ExtError, MemoryError, MessageError};
use lazy_pages::GlobalsAccessConfig;
use memory::ProcessAccessError;
use scale_info::{
    scale::{self, Decode, Encode},
    TypeInfo,
};

pub use crate::utils::TrimmedString;
pub use log;

pub const PTR_SPECIAL: u32 = u32::MAX;

#[derive(Debug, Clone, Eq, PartialEq, derive_more::From)]
pub enum TerminationReason {
    Actor(ActorTerminationReason),
    System(SystemTerminationReason),
}

impl From<PayloadSizeError> for TerminationReason {
    fn from(_err: PayloadSizeError) -> Self {
        ActorTerminationReason::Trap(TrapExplanation::Ext(
            MessageError::MaxMessageSizeExceed.into(),
        ))
        .into()
    }
}

impl From<RuntimeBufferSizeError> for TerminationReason {
    fn from(_err: RuntimeBufferSizeError) -> Self {
        ActorTerminationReason::Trap(TrapExplanation::Ext(ExtError::Memory(
            MemoryError::RuntimeAllocOutOfBounds,
        )))
        .into()
    }
}

impl From<FromUtf8Error> for TerminationReason {
    fn from(_err: FromUtf8Error) -> Self {
        ActorTerminationReason::Trap(TrapExplanation::Ext(
            ExecutionError::InvalidDebugString.into(),
        ))
        .into()
    }
}

impl From<MemoryAccessError> for TerminationReason {
    fn from(err: MemoryAccessError) -> Self {
        match err {
            MemoryAccessError::Memory(err) => TrapExplanation::Ext(err.into()).into(),
            MemoryAccessError::RuntimeBuffer(_) => {
                TrapExplanation::Ext(MemoryError::RuntimeAllocOutOfBounds.into()).into()
            }
            MemoryAccessError::Decode => unreachable!("{:?}", err),
            MemoryAccessError::GasLimitExceeded => TrapExplanation::GasLimitExceeded.into(),
            MemoryAccessError::GasAllowanceExceeded => ActorTerminationReason::GasAllowanceExceeded,
        }
        .into()
    }
}

impl From<ChargeError> for TerminationReason {
    fn from(err: ChargeError) -> Self {
        match err {
            ChargeError::GasLimitExceeded => {
                ActorTerminationReason::Trap(TrapExplanation::GasLimitExceeded).into()
            }
            ChargeError::TooManyGasAdded => SystemTerminationReason::TooManyGasAdded.into(),
            ChargeError::GasAllowanceExceeded => {
                ActorTerminationReason::GasAllowanceExceeded.into()
            }
        }
    }
}

impl<E: BackendExtError> From<E> for TerminationReason {
    fn from(err: E) -> Self {
        err.into_termination_reason()
    }
}

impl From<TrapExplanation> for TerminationReason {
    fn from(trap: TrapExplanation) -> Self {
        ActorTerminationReason::Trap(trap).into()
    }
}

#[derive(Decode, Encode, Debug, PartialEq, Eq, PartialOrd, Ord, Clone, derive_more::From)]
#[codec(crate = scale)]
pub enum ActorTerminationReason {
    Exit(ProgramId),
    Leave,
    Success,
    Wait(Option<u32>, MessageWaitedType),
    GasAllowanceExceeded,
    #[from]
    Trap(TrapExplanation),
}

#[derive(Debug, Clone, Eq, PartialEq, derive_more::Display)]
pub enum SystemTerminationReason {
    #[display(fmt = "{_0}")]
    IncorrectAllocationData(IncorrectAllocationDataError),
    #[display(fmt = "Too many gas refunded")]
    TooManyGasAdded,
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
#[codec(crate = scale)]
pub enum TrapExplanation {
    /// An error occurs in attempt to charge more gas than available during execution.
    #[display(fmt = "Not enough gas to continue execution")]
    GasLimitExceeded,
    /// An error occurs in attempt to call forbidden sys-call.
    #[display(fmt = "Unable to call a forbidden function")]
    ForbiddenFunction,
    /// The error occurs when a program tries to allocate more memory than
    /// allowed.
    #[display(fmt = "Trying to allocate more wasm program memory than allowed")]
    ProgramAllocOutOfBounds,
    #[display(fmt = "{_0}")]
    Ext(ExtError),
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

pub trait BackendExt: EnvExt + CountersOwner {
    fn into_ext_info(self, memory: &impl Memory) -> Result<ExtInfo, MemoryError>;

    fn gas_amount(&self) -> GasAmount;

    /// Pre-process memory access if need.
    fn pre_process_memory_accesses(
        reads: &[MemoryInterval],
        writes: &[MemoryInterval],
        gas_left: &mut GasLeft,
    ) -> Result<(), ProcessAccessError>;
}

pub trait BackendExtError: Clone + Sized {
    fn into_termination_reason(self) -> TerminationReason;
}

pub trait BackendAllocExtError: Sized {
    type ExtError: BackendExtError;

    fn into_backend_error(self) -> Result<Self::ExtError, Self>;
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
    #[display(fmt = "Actor backend error: {_1}")]
    Actor(GasAmount, String),
    #[display(fmt = "System backend error: {_0}")]
    System(Env),
    #[display(fmt = "Prepare error: {_1}")]
    PrepareMemory(GasAmount, PrepMem),
}

impl<Env: Display, PrepMem: Display> EnvironmentExecutionError<Env, PrepMem> {
    pub fn from_infallible(err: EnvironmentExecutionError<Env, Infallible>) -> Self {
        match err {
            EnvironmentExecutionError::System(err) => Self::System(err),
            EnvironmentExecutionError::PrepareMemory(_, err) => match err {},
            EnvironmentExecutionError::Actor(gas_amount, s) => Self::Actor(gas_amount, s),
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
        F: FnOnce(&mut Self::Memory, Option<u32>, GlobalsAccessConfig) -> Result<(), T>,
        T: Display;
}

pub trait BackendState {
    /// Set termination reason
    fn set_termination_reason(&mut self, reason: TerminationReason);

    /// Set fallible syscall error
    fn set_fallible_syscall_error(&mut self, err: ExtError);

    /// Process fallible syscall function result
    fn process_fallible_func_result<T: Sized>(
        &mut self,
        res: Result<T, TerminationReason>,
    ) -> Result<Result<T, u32>, TerminationReason> {
        match res {
            Err(err) => {
                if let TerminationReason::Actor(ActorTerminationReason::Trap(
                    TrapExplanation::Ext(ext_err),
                )) = err
                {
                    let len = ext_err.encoded_size() as u32;
                    self.set_fallible_syscall_error(ext_err);
                    Ok(Err(len))
                } else {
                    Err(err)
                }
            }
            Ok(res) => Ok(Ok(res)),
        }
    }

    /// Process alloc function result
    fn process_alloc_func_result<T: Sized, E: BackendAllocExtError>(
        &mut self,
        res: Result<T, E>,
    ) -> Result<Result<T, E>, TerminationReason> {
        match res {
            Ok(t) => Ok(Ok(t)),
            Err(err) => match err.into_backend_error() {
                Ok(ext_err) => Err(ext_err.into()),
                Err(alloc_err) => Ok(Err(alloc_err)),
            },
        }
    }
}

pub trait BackendTermination<E: BackendExt, M: Sized>: Sized {
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

        ext.set_gas_left((gas, allowance).into());

        let termination_reason = if res.is_err() {
            if matches!(
                termination_reason,
                TerminationReason::Actor(ActorTerminationReason::Success)
            ) {
                ActorTerminationReason::Trap(TrapExplanation::Unknown).into()
            } else {
                termination_reason
            }
        } else if matches!(
            termination_reason,
            TerminationReason::Actor(ActorTerminationReason::Success)
        ) {
            termination_reason
        } else {
            unreachable!(
                "Termination reason is not success, but executor successfully ends execution"
            )
        };

        (ext, memory, termination_reason)
    }
}

#[macro_export]
macro_rules! syscall_args_trace {
    ($val:expr) => {
        {
            let s = stringify!($val);
            if s.ends_with("_ptr") {
                alloc::format!(", {} = {:#x?}", s, $val)
            } else {
                alloc::format!(", {} = {:?}", s, $val)
            }
        }
    };
    ($val:expr, $($rest:expr),+) => {
        {
            let mut s = $crate::syscall_args_trace!($val);
            s.push_str(&$crate::syscall_args_trace!($($rest),+));
            s
        }
    };
}

#[macro_export]
macro_rules! syscall_trace {
    ($name:expr, $($args:expr),+) => {
        {
            $crate::log::trace!(target: "syscalls", "{}{}", $name, $crate::syscall_args_trace!($($args),+));
        }
    };
    ($name:expr) => {
        {
            $crate::log::trace!(target: "syscalls", "{}", $name);
        }
    }
}
