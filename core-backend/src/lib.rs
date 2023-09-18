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

//! Provide sp-sandbox support.

#![cfg_attr(not(feature = "std"), no_std)]

extern crate alloc;
extern crate core;

pub mod env;
mod funcs;
pub mod memory;
#[cfg(any(feature = "mock", test))]
pub mod mock;
pub mod runtime;
mod state;

use codec::{Decode, Encode};
pub use env::Environment;
pub use memory::{DefaultExecutorMemory, MemoryWrap};

use actor_system_error::actor_system_error;
use gear_core::{
    env::Externalities,
    gas::{ChargeError, CounterType, CountersOwner, GasAmount},
    ids::ProgramId,
    memory::MemoryInterval,
    message::MessageWaitedType,
    str::LimitedStr,
};
use gear_core_errors::ExtError as FallibleExtError;
use gear_lazy_pages_common::ProcessAccessError;

actor_system_error! {
    pub type TerminationReason = ActorSystemError<ActorTerminationReason, SystemTerminationReason>;
}

#[derive(Debug, Clone, Eq, PartialEq, derive_more::From)]
pub enum UndefinedTerminationReason {
    Actor(ActorTerminationReason),
    System(SystemTerminationReason),
    /// Undefined reason because we need access to counters owner trait for RI.
    ProcessAccessErrorResourcesExceed,
}

impl UndefinedTerminationReason {
    pub fn define(self, current_counter: CounterType) -> TerminationReason {
        match self {
            Self::Actor(r) => r.into(),
            Self::System(r) => r.into(),
            Self::ProcessAccessErrorResourcesExceed => {
                ActorTerminationReason::from(current_counter).into()
            }
        }
    }
}

impl From<ChargeError> for UndefinedTerminationReason {
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

impl From<TrapExplanation> for UndefinedTerminationReason {
    fn from(trap: TrapExplanation) -> Self {
        ActorTerminationReason::Trap(trap).into()
    }
}

impl<E: BackendSyscallError> From<E> for UndefinedTerminationReason {
    fn from(err: E) -> Self {
        err.into_termination_reason()
    }
}

#[derive(Decode, Encode, Debug, PartialEq, Eq, PartialOrd, Ord, Clone, derive_more::From)]
#[codec(crate = codec)]
pub enum ActorTerminationReason {
    Exit(ProgramId),
    Leave,
    Success,
    Wait(Option<u32>, MessageWaitedType),
    GasAllowanceExceeded,
    #[from]
    Trap(TrapExplanation),
}

impl From<CounterType> for ActorTerminationReason {
    fn from(counter_type: CounterType) -> Self {
        match counter_type {
            CounterType::GasLimit => Self::Trap(TrapExplanation::GasLimitExceeded),
            CounterType::GasAllowance => Self::GasAllowanceExceeded,
        }
    }
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
#[codec(crate = codec)]
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
#[codec(crate = codec)]
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
#[codec(crate = codec)]
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
#[codec(crate = codec)]
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
#[codec(crate = codec)]
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

/// Extended externalities that can manage gas counters.
pub trait BackendExternalities: Externalities + CountersOwner {
    fn gas_amount(&self) -> GasAmount;

    /// Pre-process memory access if need.
    fn pre_process_memory_accesses(
        reads: &[MemoryInterval],
        writes: &[MemoryInterval],
        gas_counter: &mut u64,
    ) -> Result<(), ProcessAccessError>;
}

/// Error returned from closure argument in [`runtime::CallerWrap::run_fallible`].
#[derive(Debug, Clone)]
pub enum RunFallibleError {
    UndefinedTerminationReason(UndefinedTerminationReason),
    FallibleExt(FallibleExtError),
}

impl<E> From<E> for RunFallibleError
where
    E: BackendSyscallError,
{
    fn from(err: E) -> Self {
        err.into_run_fallible_error()
    }
}

/// A trait for conversion of the externalities API error
/// to `UndefinedTerminationReason` and `RunFallibleError`.
pub trait BackendSyscallError: Sized {
    fn into_termination_reason(self) -> UndefinedTerminationReason;

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

#[cfg(test)]
mod tests;
