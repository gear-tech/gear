// This file is part of Gear.

// Copyright (C) 2021-2023 Gear Technologies Inc.
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

#[cfg(any(feature = "mock", test))]
pub mod mock;

pub mod funcs;
pub mod memory;
pub mod runtime;

use alloc::{
    collections::{BTreeMap, BTreeSet},
    string::String,
    vec::Vec,
};
use core::{
    convert::Infallible,
    fmt::{Debug, Display},
};
use gear_core::{
    env::Externalities,
    gas::{ChargeError, CountersOwner, GasAmount, GasLeft},
    ids::{CodeId, MessageId, ProgramId, ReservationId},
    memory::{Memory, MemoryInterval, PageBuf},
    message::{
        ContextStore, Dispatch, DispatchKind, IncomingDispatch, MessageWaitedType, WasmEntryPoint,
    },
    pages::{GearPage, WasmPage},
    reservation::GasReserver,
};
use lazy_pages::GlobalsAccessConfig;
use memory::ProcessAccessError;
use scale_info::scale::{self, Decode, Encode};

use crate::runtime::RunFallibleError;
pub use crate::utils::LimitedStr;
use gear_core::memory::MemoryError;
pub use log;

pub const PTR_SPECIAL: u32 = u32::MAX;

#[derive(Debug, Clone, Eq, PartialEq, derive_more::From)]
pub enum TerminationReason {
    Actor(ActorTerminationReason),
    System(SystemTerminationReason),
}

impl From<ChargeError> for TerminationReason {
    fn from(err: ChargeError) -> Self {
        match err {
            ChargeError::GasLimitExceeded => {
                ActorTerminationReason::Trap(TrapExplanation::GasLimitExceeded).into()
            }
            ChargeError::GasAllowanceExceeded => {
                ActorTerminationReason::GasAllowanceExceeded.into()
            }
        }
    }
}

impl From<TrapExplanation> for TerminationReason {
    fn from(trap: TrapExplanation) -> Self {
        ActorTerminationReason::Trap(trap).into()
    }
}

impl<E: BackendSyscallError> From<E> for TerminationReason {
    fn from(err: E) -> Self {
        err.into_termination_reason()
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

/// Non-actor related termination reason.
///
/// ### NOTICE:
/// It's currently unused, but is left as a stub, until
/// further massive errors refactoring is done.
#[derive(Debug, Clone, Eq, PartialEq, derive_more::Display)]
pub struct SystemTerminationReason;

/// Execution error in infallible sys-call.
#[derive(
    Decode,
    Encode,
    Debug,
    Clone,
    Eq,
    PartialEq,
    PartialOrd,
    Ord,
    derive_more::Display,
    derive_more::From,
)]
pub enum UnrecoverableExecutionError {
    #[display(fmt = "Invalid debug string passed in `gr_debug` sys-call")]
    InvalidDebugString,
    #[display(fmt = "Not enough gas for operation")]
    NotEnoughGas,
    #[display(fmt = "Length is overflowed to read payload")]
    TooBigReadLen,
    #[display(fmt = "Cannot take data in payload range from message with size")]
    ReadWrongRange,
}

/// Memory error in infallible sys-call.
#[derive(
    Decode,
    Encode,
    Debug,
    Clone,
    Eq,
    PartialEq,
    PartialOrd,
    Ord,
    derive_more::Display,
    derive_more::From,
)]
pub enum UnrecoverableMemoryError {
    /// The error occurs in attempt to access memory outside wasm program memory.
    #[display(fmt = "Trying to access memory outside wasm program memory")]
    AccessOutOfBounds,
    /// The error occurs, when program tries to allocate in block-chain runtime more memory than allowed.
    #[display(fmt = "Trying to allocate more memory in block-chain runtime than allowed")]
    RuntimeAllocOutOfBounds,
}

/// Wait error in infallible sys-call.
#[derive(
    Decode,
    Encode,
    Debug,
    Clone,
    Eq,
    PartialEq,
    PartialOrd,
    Ord,
    derive_more::Display,
    derive_more::From,
)]
pub enum UnrecoverableWaitError {
    /// An error occurs in attempt to wait for or wait up to zero blocks.
    #[display(fmt = "Waiting duration cannot be zero")]
    ZeroDuration,
    /// An error occurs in attempt to wait after reply sent.
    #[display(fmt = "`wait()` is not allowed after reply sent")]
    WaitAfterReply,
}

#[derive(
    Decode,
    Encode,
    Debug,
    Clone,
    Eq,
    PartialEq,
    PartialOrd,
    Ord,
    derive_more::Display,
    derive_more::From,
)]
pub enum UnrecoverableExtError {
    #[display(fmt = "Execution error: {_0}")]
    Execution(UnrecoverableExecutionError),
    #[display(fmt = "Memory error: {_0}")]
    Memory(UnrecoverableMemoryError),
    #[display(fmt = "Waiting error: {_0}")]
    Wait(UnrecoverableWaitError),
}

#[derive(
    Decode,
    Encode,
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
    #[display(fmt = "Sys-call unrecoverable error: {_0}")]
    UnrecoverableExt(UnrecoverableExtError),
    #[display(fmt = "{_0}")]
    Panic(LimitedStr<'static>),
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
    pub reply_deposits: Vec<(MessageId, u64)>,
    pub program_candidates_data: BTreeMap<CodeId, Vec<(MessageId, ProgramId)>>,
    pub program_rents: BTreeMap<ProgramId, u32>,
    pub context_store: ContextStore,
}

/// Extended externalities that can manage gas counters.
pub trait BackendExternalities: Externalities + CountersOwner {
    fn into_ext_info(self, memory: &impl Memory) -> Result<ExtInfo, MemoryError>;

    fn gas_amount(&self) -> GasAmount;

    /// Pre-process memory access if need.
    fn pre_process_memory_accesses(
        reads: &[MemoryInterval],
        writes: &[MemoryInterval],
        gas_left: &mut GasLeft,
    ) -> Result<(), ProcessAccessError>;
}

/// A trait for conversion of the externalities API error
/// to `TerminationReason` and `RunFallibleError`.
pub trait BackendSyscallError: Sized {
    fn into_termination_reason(self) -> TerminationReason;

    fn into_run_fallible_error(self) -> RunFallibleError;
}

// TODO: consider to remove this trait and use Result<Result<Page, AllocError>, GasError> instead #2571
/// A trait for conversion of the externalities memory management error to api error.
///
/// If the conversion fails, then `Self` is returned in the `Err` variant.
pub trait BackendAllocSyscallError: Sized {
    type ExtError: BackendSyscallError;

    fn into_backend_error(self) -> Result<Self::ExtError, Self>;
}

pub struct BackendReport<EnvMem, Ext>
where
    Ext: Externalities,
{
    pub termination_reason: TerminationReason,
    pub memory_wrap: EnvMem,
    pub ext: Ext,
}

#[derive(Debug, derive_more::Display)]
pub enum EnvironmentError<EnvSystemError: Display, PrepareMemoryError: Display> {
    #[display(fmt = "Actor backend error: {_1}")]
    Actor(GasAmount, String),
    #[display(fmt = "System backend error: {_0}")]
    System(EnvSystemError),
    #[display(fmt = "Prepare error: {_1}")]
    PrepareMemory(GasAmount, PrepareMemoryError),
}

impl<EnvSystemError: Display, PrepareMemoryError: Display>
    EnvironmentError<EnvSystemError, PrepareMemoryError>
{
    pub fn from_infallible(err: EnvironmentError<EnvSystemError, Infallible>) -> Self {
        match err {
            EnvironmentError::System(err) => Self::System(err),
            EnvironmentError::PrepareMemory(_, err) => match err {},
            EnvironmentError::Actor(gas_amount, s) => Self::Actor(gas_amount, s),
        }
    }
}

type EnvironmentBackendReport<Env, EntryPoint> =
    BackendReport<<Env as Environment<EntryPoint>>::Memory, <Env as Environment<EntryPoint>>::Ext>;

pub type EnvironmentExecutionResult<PrepareMemoryError, Env, EntryPoint> = Result<
    EnvironmentBackendReport<Env, EntryPoint>,
    EnvironmentError<<Env as Environment<EntryPoint>>::SystemError, PrepareMemoryError>,
>;

pub trait Environment<EntryPoint = DispatchKind>: Sized
where
    EntryPoint: WasmEntryPoint,
{
    type Ext: BackendExternalities + 'static;

    /// Memory type for current environment.
    type Memory: Memory;

    /// That's an error which originally comes from the primary
    /// wasm execution environment (set by wasmi or sandbox).
    /// So it's not the error of the `Self` itself, it's a kind
    /// of wrapper over the underlying executor error.
    type SystemError: Debug + Display;

    /// 1) Instantiates wasm binary.
    /// 2) Creates wasm memory
    /// 3) Runs `prepare_memory` to fill the memory before running instance.
    /// 4) Instantiate external funcs for wasm module.
    fn new(
        ext: Self::Ext,
        binary: &[u8],
        entry_point: EntryPoint,
        entries: BTreeSet<DispatchKind>,
        mem_size: WasmPage,
    ) -> Result<Self, EnvironmentError<Self::SystemError, Infallible>>;

    /// Run instance setup starting at `entry_point` - wasm export function name.
    fn execute<PrepareMemory, PrepareMemoryError>(
        self,
        prepare_memory: PrepareMemory,
    ) -> EnvironmentExecutionResult<PrepareMemoryError, Self, EntryPoint>
    where
        PrepareMemory: FnOnce(
            &mut Self::Memory,
            Option<u32>,
            GlobalsAccessConfig,
        ) -> Result<(), PrepareMemoryError>,
        PrepareMemoryError: Display;
}

pub trait BackendState {
    /// Set termination reason
    fn set_termination_reason(&mut self, reason: TerminationReason);

    /// Process fallible syscall function result
    fn process_fallible_func_result<T: Sized>(
        &mut self,
        res: Result<T, RunFallibleError>,
    ) -> Result<Result<T, u32>, TerminationReason> {
        match res {
            Err(RunFallibleError::FallibleExt(ext_err)) => {
                let code = ext_err.to_u32();
                log::trace!(target: "syscalls", "fallible syscall error: {ext_err}");
                Ok(Err(code))
            }
            Err(RunFallibleError::TerminationReason(reason)) => Err(reason),
            Ok(res) => Ok(Ok(res)),
        }
    }

    /// Process alloc function result
    fn process_alloc_func_result<T: Sized, ExtAllocError: BackendAllocSyscallError>(
        &mut self,
        res: Result<T, ExtAllocError>,
    ) -> Result<Result<T, ExtAllocError>, TerminationReason> {
        match res {
            Ok(t) => Ok(Ok(t)),
            Err(err) => match err.into_backend_error() {
                Ok(ext_err) => Err(ext_err.into()),
                Err(alloc_err) => Ok(Err(alloc_err)),
            },
        }
    }
}

/// A trait for termination of the gear sys-calls execution backend.
///
/// Backend termination aims to return to the caller gear wasm program
/// execution outcome, which is the state of externalities, memory and
/// termination reason.
pub trait BackendTermination<Ext: BackendExternalities, EnvMem: Sized>: Sized {
    /// Transforms [`Self`] into tuple of externalities, memory and
    /// termination reason returned after the execution.
    fn into_parts(self) -> (Ext, EnvMem, TerminationReason);

    /// Terminates backend work after execution.
    ///
    /// The function handles `res`, which is the result of gear wasm
    /// program entry point invocation, and the termination reason.
    ///
    /// If the `res` is `Ok`, then execution considered successful
    /// and the termination reason will have the corresponding value.
    ///
    /// If the `res` is `Err`, then execution is considered to end
    /// with an error and the actual termination reason, which stores
    /// more precise information about the error, is returned.
    ///
    /// There's a case, when `res` is `Err`, but termination reason has
    /// a value for the successful ending of the execution. This is the
    /// case of calling `unreachable` panic in the program.
    fn terminate<T: Debug, WasmCallErr: Debug>(
        self,
        res: Result<T, WasmCallErr>,
        gas: i64,
        allowance: i64,
    ) -> (Ext, EnvMem, TerminationReason) {
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

#[cfg(test)]
mod tests;
