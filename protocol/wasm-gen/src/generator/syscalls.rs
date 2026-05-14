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

//! Syscalls generators entities.
//!
//! Generators from this module form a state machine:
//! ```text
//! # Zero syscalls generators nesting level.
//! SyscallsImportGenerator--->DisabledSyscallsImportsGenerator--->ModuleWithCallIndexes--->WasmModule
//!
//! # First syscalls generators nesting level.
//! SyscallsImportGenerator--->DisabledSyscallsImportsGenerator--(SyscallsImportsGenerationProof)-->AdditionalDataInjector---\
//! |--->DisabledAdditionalDataInjector--->ModuleWithCallIndexes--->WasmModule
//!
//! SyscallsImportGenerator--->DisabledSyscallsImportsGenerator--(SyscallsImportsGenerationProof)-->SyscallsInvocator---\
//! |--->DisabledSyscallsInvocator--->ModuleWithCallIndexes--->WasmModule
//!
//! # Second syscalls generators nesting level
//! SyscallsImportGenerator--->DisabledSyscallsImportsGenerator--(SyscallsImportsGenerationProof)-->AdditionalDataInjector---\
//! |--->DisabledAdditionalDataInjector-->SyscallsInvocator--->DisabledSyscallsInvocator--->ModuleWithCallIndexes--->WasmModule
//! ```
//! Entities in curly brackets are those, which are required for the next transition.
//! Also all transitions require previous entity to be disabled.

mod additional_data;
mod imports;
mod invocator;

pub use additional_data::*;
pub use imports::*;
pub use invocator::*;

use gear_wasm_instrument::syscalls::{
    ErrPtr, HashType, Ptr, RegularParamType, SyscallName, SyscallSignature,
};

/// Type of invocable syscall.
///
/// Basically, there are 2 types of generated syscalls:
/// 1. Those invocation of which is done regardless of validity of call context (`Loose`).
/// 2. Those which are invoked correctly with implementing all call context (`Precise`).
///
/// Clarifying that, `gr_reservation_send` requires an existing reservation id,
/// which is pretty hard to predict beforehand with a generator. So this call context
/// is created from scratch - first `gr_reserve_gas` is called and then it's result
/// is used for the further `gr_reservation_send` call. Those are `Precise` syscalls.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum InvocableSyscall {
    Loose(SyscallName),
    Precise(SyscallName),
}

impl InvocableSyscall {
    pub(crate) fn to_str(self) -> &'static str {
        match self {
            InvocableSyscall::Loose(syscall) => syscall.to_str(),
            InvocableSyscall::Precise(syscall) => match syscall {
                SyscallName::ReservationSend => "precise_gr_reservation_send",
                SyscallName::ReservationReply => "precise_gr_reservation_reply",
                SyscallName::SendCommit => "precise_gr_send_commit",
                SyscallName::SendCommitWGas => "precise_gr_send_commit_wgas",
                SyscallName::ReplyDeposit => "precise_gr_reply_deposit",
                _ => unimplemented!(),
            },
        }
    }

    fn into_signature(self) -> SyscallSignature {
        match self {
            InvocableSyscall::Loose(name) => name.signature(),
            InvocableSyscall::Precise(name) => match name {
                SyscallName::ReservationSend => SyscallSignature::gr_fallible((
                    [
                        // Address of recipient and value (HashWithValue struct)
                        Ptr::HashWithValue(HashType::ActorId).into(),
                        // Pointer to payload
                        Ptr::SizedBufferStart {
                            length_param_idx: 2,
                        }
                        .into(),
                        // Length of the payload
                        RegularParamType::Length,
                        // Number of blocks to delay the sending for
                        RegularParamType::DelayBlockNumber,
                        // Amount of gas to reserve
                        RegularParamType::Gas,
                        // Duration of the reservation
                        RegularParamType::DurationBlockNumber,
                    ],
                    // Address of error returned
                    ErrPtr::ErrorWithHash(HashType::MessageId),
                )),
                SyscallName::ReservationReply => SyscallSignature::gr_fallible((
                    [
                        // Address of value
                        Ptr::Value.into(),
                        // Pointer to payload
                        Ptr::SizedBufferStart {
                            length_param_idx: 2,
                        }
                        .into(),
                        // Length of the payload
                        RegularParamType::Length,
                        // Amount of gas to reserve
                        RegularParamType::Gas,
                        // Duration of the reservation
                        RegularParamType::DurationBlockNumber,
                    ],
                    // Address of error returned
                    ErrPtr::ErrorWithHash(HashType::MessageId),
                )),
                SyscallName::SendCommit => SyscallSignature::gr_fallible((
                    [
                        // Address of recipient and value (HashWithValue struct)
                        Ptr::HashWithValue(HashType::ActorId).into(),
                        // Pointer to payload
                        Ptr::SizedBufferStart {
                            length_param_idx: 2,
                        }
                        .into(),
                        // Length of the payload
                        RegularParamType::Length,
                        // Number of blocks to delay the sending for
                        RegularParamType::DelayBlockNumber,
                    ],
                    // Address of error returned, `ErrorCode` here because underlying syscalls have different error types
                    ErrPtr::ErrorCode,
                )),
                SyscallName::SendCommitWGas => SyscallSignature::gr_fallible((
                    [
                        // Address of recipient and value (HashWithValue struct)
                        Ptr::HashWithValue(HashType::ActorId).into(),
                        // Number of blocks to delay the sending for
                        RegularParamType::DelayBlockNumber,
                        // Amount of gas to reserve
                        RegularParamType::Gas,
                    ],
                    // Address of error returned, `ErrorCode` here because underlying syscalls have different error types
                    ErrPtr::ErrorCode,
                )),
                SyscallName::ReplyDeposit => SyscallSignature::gr_fallible((
                    [
                        // Address of recipient and value (HashWithValue struct). That's needed
                        // because first `gr_send_input` is invoked and resulting message id is
                        // used as an input to `gr_reply_deposit`.
                        Ptr::HashWithValue(HashType::ActorId).into(),
                        // An offset defining starting index in the received payload (related to `gr_send_input`).
                        RegularParamType::Offset,
                        // Length of the slice of the received message payload (related to `gr_send_input`).
                        RegularParamType::Length,
                        // Delay (related to `gr_send_input`).
                        RegularParamType::DelayBlockNumber,
                        // Amount of gas deposited for a message id got from `gr_send_input`.
                        // That's an actual input for `gr_reply_deposit`
                        RegularParamType::Gas,
                    ],
                    // Error pointer
                    ErrPtr::ErrorWithHash(HashType::MessageId),
                )),
                _ => unimplemented!(),
            },
        }
    }

    /// Checks whether given syscall has the precise variant.
    pub(crate) fn has_precise_variant(syscall: SyscallName) -> bool {
        Self::required_imports_for_syscall(syscall).is_some()
    }

    /// Returns the required imports to build precise syscall, but of a fixed size.
    fn required_imports<const N: usize>(syscall: SyscallName) -> &'static [SyscallName; N] {
        Self::required_imports_for_syscall(syscall)
            .expect("failed to find required imports for syscall")
            .try_into()
            .expect("failed to convert slice")
    }

    /// Returns the required imports to build precise syscall.
    pub(crate) fn required_imports_for_syscall(
        syscall: SyscallName,
    ) -> Option<&'static [SyscallName]> {
        // NOTE: the last syscall must be pattern itself
        Some(match syscall {
            SyscallName::ReservationSend => {
                &[SyscallName::ReserveGas, SyscallName::ReservationSend]
            }
            SyscallName::ReservationReply => {
                &[SyscallName::ReserveGas, SyscallName::ReservationReply]
            }
            SyscallName::SendCommit => &[
                SyscallName::SendInit,
                SyscallName::SendPush,
                SyscallName::SendCommit,
            ],
            SyscallName::SendCommitWGas => &[
                SyscallName::Size,
                SyscallName::SendInit,
                SyscallName::SendPushInput,
                SyscallName::SendCommitWGas,
            ],
            SyscallName::ReplyDeposit => &[SyscallName::SendInput, SyscallName::ReplyDeposit],
            // Enable `MessageId` syscall import if `Wake` syscall is enabled.
            // This is done to provide a syscall `Wake` with a correct message id.
            SyscallName::Wake => &[SyscallName::MessageId],
            _ => return None,
        })
    }

    /// Checks whether syscall is error-prone either by returning error indicating value
    /// or by providing error pointer as a syscall param.
    ///
    /// There are only 2 syscalls returning error value: `Alloc` and `Free`.
    ///
    /// If syscall changes from fallible into infallible or vice versa in future,
    /// we'll see it by analyzing code coverage stats produced by fuzzer.
    pub(crate) fn returns_error(&self) -> bool {
        match self {
            InvocableSyscall::Loose(syscall) => syscall.returns_error(),
            InvocableSyscall::Precise(syscall) => syscall.returns_error(),
        }
    }

    #[cfg(test)]
    pub(crate) fn is_fallible(&self) -> bool {
        match self {
            InvocableSyscall::Loose(syscall) => syscall.is_fallible(),
            InvocableSyscall::Precise(syscall) => syscall.is_fallible(),
        }
    }
}
