// This file is part of Gear.

// Copyright (C) 2022-2023 Gear Technologies Inc.
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

//! Gear syscalls for smart contracts execution signatures.

use crate::parity_wasm::elements::{FunctionType, ValueType};
use alloc::{collections::BTreeSet, vec::Vec};
use enum_iterator::{self, Sequence};

/// All available syscalls.
///
/// The type is mainly used to prevent from skipping syscall integration test for
/// a newly introduced syscall or from typo in syscall name.
#[derive(Debug, Clone, Copy, Ord, PartialOrd, Eq, PartialEq, Sequence, Hash)]
pub enum SyscallName {
    // Message sending related
    // --
    // Sending `handle` message
    Send,
    SendWGas,
    SendCommit,
    SendCommitWGas,
    SendInit,
    SendPush,
    ReservationSend,
    ReservationSendCommit,
    SendInput,
    SendPushInput,
    SendInputWGas,

    // Sending `handle_reply` message
    Reply,
    ReplyWGas,
    ReplyCommit,
    ReplyCommitWGas,
    ReplyPush,
    ReservationReply,
    ReservationReplyCommit,
    ReplyInput,
    ReplyPushInput,
    ReplyInputWGas,

    // Sending `init` message
    CreateProgram,
    CreateProgramWGas,

    // Message data related
    Read,
    ReplyTo,
    SignalFrom,
    Size,
    ReplyCode,
    SignalCode,
    MessageId,
    ProgramId,
    Source,
    Value,

    // Program execution related
    // --
    // Execution environmental data
    EnvVars,
    BlockHeight,
    BlockTimestamp,
    GasAvailable,
    ValueAvailable,

    // Changing execution path calls
    Exit,
    Leave,
    Wait,
    WaitFor,
    WaitUpTo,
    Wake,
    Panic,
    OomPanic,

    // Hard under the hood calls, serving proper program execution
    Alloc,
    Free,
    OutOfGas,

    // Miscellaneous
    ReplyDeposit,
    Debug,
    Random,
    ReserveGas,
    UnreserveGas,
    SystemReserveGas,
    PayProgramRent,
}

impl SyscallName {
    pub fn to_str(&self) -> &'static str {
        match self {
            SyscallName::Alloc => "alloc",
            SyscallName::EnvVars => "gr_env_vars",
            SyscallName::BlockHeight => "gr_block_height",
            SyscallName::BlockTimestamp => "gr_block_timestamp",
            SyscallName::CreateProgram => "gr_create_program",
            SyscallName::CreateProgramWGas => "gr_create_program_wgas",
            SyscallName::ReplyDeposit => "gr_reply_deposit",
            SyscallName::Debug => "gr_debug",
            SyscallName::Panic => "gr_panic",
            SyscallName::OomPanic => "gr_oom_panic",
            SyscallName::Exit => "gr_exit",
            SyscallName::Free => "free",
            SyscallName::GasAvailable => "gr_gas_available",
            SyscallName::Leave => "gr_leave",
            SyscallName::MessageId => "gr_message_id",
            SyscallName::OutOfGas => "gr_out_of_gas",
            SyscallName::PayProgramRent => "gr_pay_program_rent",
            SyscallName::ProgramId => "gr_program_id",
            SyscallName::Random => "gr_random",
            SyscallName::Read => "gr_read",
            SyscallName::Reply => "gr_reply",
            SyscallName::ReplyCommit => "gr_reply_commit",
            SyscallName::ReplyCommitWGas => "gr_reply_commit_wgas",
            SyscallName::ReplyPush => "gr_reply_push",
            SyscallName::ReplyTo => "gr_reply_to",
            SyscallName::SignalFrom => "gr_signal_from",
            SyscallName::ReplyWGas => "gr_reply_wgas",
            SyscallName::ReplyInput => "gr_reply_input",
            SyscallName::ReplyPushInput => "gr_reply_push_input",
            SyscallName::ReplyInputWGas => "gr_reply_input_wgas",
            SyscallName::ReservationReply => "gr_reservation_reply",
            SyscallName::ReservationReplyCommit => "gr_reservation_reply_commit",
            SyscallName::ReservationSend => "gr_reservation_send",
            SyscallName::ReservationSendCommit => "gr_reservation_send_commit",
            SyscallName::ReserveGas => "gr_reserve_gas",
            SyscallName::Send => "gr_send",
            SyscallName::SendCommit => "gr_send_commit",
            SyscallName::SendCommitWGas => "gr_send_commit_wgas",
            SyscallName::SendInit => "gr_send_init",
            SyscallName::SendPush => "gr_send_push",
            SyscallName::SendWGas => "gr_send_wgas",
            SyscallName::SendInput => "gr_send_input",
            SyscallName::SendPushInput => "gr_send_push_input",
            SyscallName::SendInputWGas => "gr_send_input_wgas",
            SyscallName::Size => "gr_size",
            SyscallName::Source => "gr_source",
            SyscallName::ReplyCode => "gr_reply_code",
            SyscallName::SignalCode => "gr_signal_code",
            SyscallName::SystemReserveGas => "gr_system_reserve_gas",
            SyscallName::UnreserveGas => "gr_unreserve_gas",
            SyscallName::Value => "gr_value",
            SyscallName::ValueAvailable => "gr_value_available",
            SyscallName::Wait => "gr_wait",
            SyscallName::WaitFor => "gr_wait_for",
            SyscallName::WaitUpTo => "gr_wait_up_to",
            SyscallName::Wake => "gr_wake",
        }
    }

    pub fn all() -> impl Iterator<Item = Self> {
        enum_iterator::all()
    }

    pub fn count() -> usize {
        Self::all().count()
    }

    /// Returns list of all syscall names (actually supported by this module syscalls).
    pub fn instrumentable() -> BTreeSet<Self> {
        [
            Self::Alloc,
            Self::Free,
            Self::Debug,
            Self::Panic,
            Self::OomPanic,
            Self::BlockHeight,
            Self::BlockTimestamp,
            Self::Exit,
            Self::GasAvailable,
            Self::PayProgramRent,
            Self::ProgramId,
            Self::Leave,
            Self::ValueAvailable,
            Self::Wait,
            Self::WaitUpTo,
            Self::WaitFor,
            Self::Wake,
            Self::ReplyCode,
            Self::SignalCode,
            Self::MessageId,
            Self::Read,
            Self::Reply,
            Self::ReplyWGas,
            Self::ReplyInput,
            Self::ReplyInputWGas,
            Self::ReplyCommit,
            Self::ReplyCommitWGas,
            Self::ReservationReply,
            Self::ReservationReplyCommit,
            Self::ReplyPush,
            Self::ReplyPushInput,
            Self::ReplyTo,
            Self::SignalFrom,
            Self::Send,
            Self::SendWGas,
            Self::SendInput,
            Self::SendInputWGas,
            Self::SendCommit,
            Self::SendCommitWGas,
            Self::SendInit,
            Self::SendPush,
            Self::SendPushInput,
            Self::ReservationSend,
            Self::ReservationSendCommit,
            Self::Size,
            Self::Source,
            Self::Value,
            Self::CreateProgram,
            Self::CreateProgramWGas,
            Self::ReplyDeposit,
            Self::ReserveGas,
            Self::UnreserveGas,
            Self::Random,
        ]
        .into()
    }

    /// Returns signature for syscall by name.
    pub fn signature(self) -> SyscallSignature {
        use ParamType::*;
        use ValueType::I32;
        match self {
            Self::Alloc => SyscallSignature::system([Alloc], [I32]),
            Self::Free => SyscallSignature::system([Free], [I32]),
            Self::Debug => SyscallSignature::gr_infallible([
                Ptr(PtrInfo::new_immutable(PtrType::SizedBufferStart {
                    length_param_idx: 1,
                })),
                Length,
            ]),
            Self::Panic => SyscallSignature::gr_infallible([
                Ptr(PtrInfo::new_immutable(PtrType::SizedBufferStart {
                    length_param_idx: 1,
                })),
                Length,
            ]),
            Self::OomPanic => SyscallSignature::gr_infallible([]),
            Self::BlockHeight => {
                SyscallSignature::gr_infallible([Ptr(PtrInfo::new_mutable(PtrType::BlockNumber))])
            }
            Self::BlockTimestamp => SyscallSignature::gr_infallible([Ptr(PtrInfo::new_mutable(
                PtrType::BlockTimestamp,
            ))]),
            Self::Exit => SyscallSignature::gr_infallible([Ptr(PtrInfo::new_immutable(
                PtrType::Hash(HashType::ActorId),
            ))]),
            Self::GasAvailable => {
                SyscallSignature::gr_infallible([Ptr(PtrInfo::new_mutable(PtrType::Gas))])
            }
            Self::PayProgramRent => SyscallSignature::gr_fallible([
                Ptr(PtrInfo::new_immutable(PtrType::HashWithValue(
                    HashType::ActorId,
                ))),
                Ptr(PtrInfo::new_mutable(PtrType::ErrorWithBlockNumberAndValue)),
            ]),
            Self::ProgramId => SyscallSignature::gr_infallible([Ptr(PtrInfo::new_mutable(
                PtrType::Hash(HashType::ActorId),
            ))]),
            Self::Leave => SyscallSignature::gr_infallible([]),
            Self::ValueAvailable => {
                SyscallSignature::gr_infallible([Ptr(PtrInfo::new_mutable(PtrType::Value))])
            }
            Self::Wait => SyscallSignature::gr_infallible([]),
            Self::WaitUpTo => SyscallSignature::gr_infallible([DurationBlockNumber]),
            Self::WaitFor => SyscallSignature::gr_infallible([DurationBlockNumber]),
            Self::Wake => SyscallSignature::gr_fallible([
                Ptr(PtrInfo::new_immutable(PtrType::Hash(HashType::MessageId))),
                DelayBlockNumber,
                Ptr(PtrInfo::new_mutable(PtrType::ErrorCode)),
            ]),
            Self::ReplyCode => SyscallSignature::gr_fallible([Ptr(PtrInfo::new_mutable(
                PtrType::ErrorWithReplyCode,
            ))]),
            Self::SignalCode => SyscallSignature::gr_fallible([Ptr(PtrInfo::new_mutable(
                PtrType::ErrorWithSignalCode,
            ))]),
            Self::MessageId => SyscallSignature::gr_infallible([Ptr(PtrInfo::new_mutable(
                PtrType::Hash(HashType::MessageId),
            ))]),
            Self::EnvVars => SyscallSignature::gr_infallible([
                Version,
                Ptr(PtrInfo::new_mutable(PtrType::BufferStart)),
            ]),
            Self::Read => SyscallSignature::gr_fallible([
                Offset,
                Length,
                Ptr(PtrInfo::new_mutable(PtrType::SizedBufferStart {
                    length_param_idx: 1,
                })),
                Ptr(PtrInfo::new_mutable(PtrType::ErrorCode)),
            ]),
            Self::Reply => SyscallSignature::gr_fallible([
                Ptr(PtrInfo::new_immutable(PtrType::SizedBufferStart {
                    length_param_idx: 1,
                })),
                Length,
                Ptr(PtrInfo::new_immutable(PtrType::Value)),
                Ptr(PtrInfo::new_mutable(PtrType::ErrorWithHash(
                    HashType::MessageId,
                ))),
            ]),
            Self::ReplyInput => SyscallSignature::gr_fallible([
                Offset,
                Length,
                Ptr(PtrInfo::new_immutable(PtrType::Value)),
                Ptr(PtrInfo::new_mutable(PtrType::ErrorWithHash(
                    HashType::MessageId,
                ))),
            ]),
            Self::ReplyWGas => SyscallSignature::gr_fallible([
                Ptr(PtrInfo::new_immutable(PtrType::SizedBufferStart {
                    length_param_idx: 1,
                })),
                Length,
                Gas,
                Ptr(PtrInfo::new_immutable(PtrType::Value)),
                Ptr(PtrInfo::new_mutable(PtrType::ErrorWithHash(
                    HashType::MessageId,
                ))),
            ]),
            Self::ReplyInputWGas => SyscallSignature::gr_fallible([
                Offset,
                Length,
                Gas,
                Ptr(PtrInfo::new_immutable(PtrType::Value)),
                Ptr(PtrInfo::new_mutable(PtrType::ErrorWithHash(
                    HashType::MessageId,
                ))),
            ]),
            Self::ReplyCommit => SyscallSignature::gr_fallible([
                Ptr(PtrInfo::new_immutable(PtrType::Value)),
                Ptr(PtrInfo::new_mutable(PtrType::ErrorWithHash(
                    HashType::MessageId,
                ))),
            ]),
            Self::ReplyCommitWGas => SyscallSignature::gr_fallible([
                Gas,
                Ptr(PtrInfo::new_immutable(PtrType::Value)),
                Ptr(PtrInfo::new_mutable(PtrType::ErrorWithHash(
                    HashType::MessageId,
                ))),
            ]),
            Self::ReservationReply => SyscallSignature::gr_fallible([
                Ptr(PtrInfo::new_immutable(PtrType::HashWithValue(
                    HashType::ReservationId,
                ))),
                Ptr(PtrInfo::new_immutable(PtrType::SizedBufferStart {
                    length_param_idx: 2,
                })),
                Length,
                Ptr(PtrInfo::new_mutable(PtrType::ErrorWithHash(
                    HashType::MessageId,
                ))),
            ]),
            Self::ReservationReplyCommit => SyscallSignature::gr_fallible([
                Ptr(PtrInfo::new_immutable(PtrType::HashWithValue(
                    HashType::ReservationId,
                ))),
                Ptr(PtrInfo::new_mutable(PtrType::ErrorWithHash(
                    HashType::MessageId,
                ))),
            ]),
            Self::ReplyPush => SyscallSignature::gr_fallible([
                Ptr(PtrInfo::new_immutable(PtrType::SizedBufferStart {
                    length_param_idx: 1,
                })),
                Length,
                Ptr(PtrInfo::new_mutable(PtrType::ErrorCode)),
            ]),
            Self::ReplyPushInput => SyscallSignature::gr_fallible([
                Offset,
                Length,
                Ptr(PtrInfo::new_mutable(PtrType::ErrorCode)),
            ]),
            Self::ReplyTo => SyscallSignature::gr_fallible([Ptr(PtrInfo::new_mutable(
                PtrType::ErrorWithHash(HashType::MessageId),
            ))]),
            Self::SignalFrom => SyscallSignature::gr_fallible([Ptr(PtrInfo::new_mutable(
                PtrType::ErrorWithHash(HashType::MessageId),
            ))]),
            Self::Send => SyscallSignature::gr_fallible([
                Ptr(PtrInfo::new_immutable(PtrType::HashWithValue(
                    HashType::ActorId,
                ))),
                Ptr(PtrInfo::new_immutable(PtrType::SizedBufferStart {
                    length_param_idx: 2,
                })),
                Length,
                DelayBlockNumber,
                Ptr(PtrInfo::new_mutable(PtrType::ErrorWithHash(
                    HashType::MessageId,
                ))),
            ]),
            Self::SendInput => SyscallSignature::gr_fallible([
                Ptr(PtrInfo::new_immutable(PtrType::HashWithValue(
                    HashType::ActorId,
                ))),
                Offset,
                Length,
                DelayBlockNumber,
                Ptr(PtrInfo::new_mutable(PtrType::ErrorWithHash(
                    HashType::MessageId,
                ))),
            ]),
            Self::SendWGas => SyscallSignature::gr_fallible([
                Ptr(PtrInfo::new_immutable(PtrType::HashWithValue(
                    HashType::ActorId,
                ))),
                Ptr(PtrInfo::new_immutable(PtrType::SizedBufferStart {
                    length_param_idx: 2,
                })),
                Length,
                Gas,
                DelayBlockNumber,
                Ptr(PtrInfo::new_mutable(PtrType::ErrorWithHash(
                    HashType::MessageId,
                ))),
            ]),
            Self::SendInputWGas => SyscallSignature::gr_fallible([
                Ptr(PtrInfo::new_immutable(PtrType::HashWithValue(
                    HashType::ActorId,
                ))),
                Offset,
                Length,
                Gas,
                DelayBlockNumber,
                Ptr(PtrInfo::new_mutable(PtrType::ErrorWithHash(
                    HashType::MessageId,
                ))),
            ]),
            Self::SendCommit => SyscallSignature::gr_fallible([
                Handler,
                Ptr(PtrInfo::new_immutable(PtrType::HashWithValue(
                    HashType::ActorId,
                ))),
                DelayBlockNumber,
                Ptr(PtrInfo::new_mutable(PtrType::ErrorWithHash(
                    HashType::MessageId,
                ))),
            ]),
            Self::SendCommitWGas => SyscallSignature::gr_fallible([
                Handler,
                Ptr(PtrInfo::new_immutable(PtrType::HashWithValue(
                    HashType::ActorId,
                ))),
                Gas,
                DelayBlockNumber,
                Ptr(PtrInfo::new_mutable(PtrType::ErrorWithHash(
                    HashType::MessageId,
                ))),
            ]),
            Self::SendInit => {
                SyscallSignature::gr_fallible([Ptr(PtrInfo::new_mutable(PtrType::ErrorWithHandle))])
            }
            Self::SendPush => SyscallSignature::gr_fallible([
                Handler,
                Ptr(PtrInfo::new_immutable(PtrType::SizedBufferStart {
                    length_param_idx: 2,
                })),
                Length,
                Ptr(PtrInfo::new_mutable(PtrType::ErrorCode)),
            ]),
            Self::SendPushInput => SyscallSignature::gr_fallible([
                Handler,
                Offset,
                Length,
                Ptr(PtrInfo::new_mutable(PtrType::ErrorCode)),
            ]),
            Self::ReservationSend => SyscallSignature::gr_fallible([
                Ptr(PtrInfo::new_immutable(PtrType::TwoHashesWithValue(
                    HashType::ReservationId,
                    HashType::ActorId,
                ))),
                Ptr(PtrInfo::new_immutable(PtrType::SizedBufferStart {
                    length_param_idx: 2,
                })),
                Length,
                DelayBlockNumber,
                Ptr(PtrInfo::new_mutable(PtrType::ErrorWithHash(
                    HashType::MessageId,
                ))),
            ]),
            Self::ReservationSendCommit => SyscallSignature::gr_fallible([
                Handler,
                Ptr(PtrInfo::new_immutable(PtrType::TwoHashesWithValue(
                    HashType::ReservationId,
                    HashType::ActorId,
                ))),
                DelayBlockNumber,
                Ptr(PtrInfo::new_mutable(PtrType::ErrorWithHash(
                    HashType::MessageId,
                ))),
            ]),
            Self::Size => {
                SyscallSignature::gr_infallible([Ptr(PtrInfo::new_mutable(PtrType::Length))])
            }
            Self::Source => SyscallSignature::gr_infallible([Ptr(PtrInfo::new_mutable(
                PtrType::Hash(HashType::ActorId),
            ))]),
            Self::Value => {
                SyscallSignature::gr_infallible([Ptr(PtrInfo::new_mutable(PtrType::Value))])
            }
            Self::CreateProgram => SyscallSignature::gr_fallible([
                Ptr(PtrInfo::new_immutable(PtrType::HashWithValue(
                    HashType::CodeId,
                ))),
                Ptr(PtrInfo::new_immutable(PtrType::SizedBufferStart {
                    length_param_idx: 2,
                })),
                Length,
                Ptr(PtrInfo::new_immutable(PtrType::SizedBufferStart {
                    length_param_idx: 4,
                })),
                Length,
                DelayBlockNumber,
                Ptr(PtrInfo::new_mutable(PtrType::ErrorWithTwoHashes(
                    HashType::MessageId,
                    HashType::ActorId,
                ))),
            ]),
            Self::CreateProgramWGas => SyscallSignature::gr_fallible([
                Ptr(PtrInfo::new_immutable(PtrType::HashWithValue(
                    HashType::CodeId,
                ))),
                Ptr(PtrInfo::new_immutable(PtrType::SizedBufferStart {
                    length_param_idx: 2,
                })),
                Length,
                Ptr(PtrInfo::new_immutable(PtrType::SizedBufferStart {
                    length_param_idx: 4,
                })),
                Length,
                Gas,
                DelayBlockNumber,
                Ptr(PtrInfo::new_mutable(PtrType::ErrorWithTwoHashes(
                    HashType::MessageId,
                    HashType::ActorId,
                ))),
            ]),
            Self::ReplyDeposit => SyscallSignature::gr_fallible([
                Ptr(PtrInfo::new_immutable(PtrType::Hash(HashType::MessageId))),
                Gas,
                Ptr(PtrInfo::new_mutable(PtrType::ErrorCode)),
            ]),
            Self::ReserveGas => SyscallSignature::gr_fallible([
                Gas,
                DurationBlockNumber,
                Ptr(PtrInfo::new_mutable(PtrType::ErrorWithHash(
                    HashType::ReservationId,
                ))),
            ]),
            Self::UnreserveGas => SyscallSignature::gr_fallible([
                Ptr(PtrInfo::new_immutable(PtrType::Hash(
                    HashType::ReservationId,
                ))),
                Ptr(PtrInfo::new_mutable(PtrType::ErrorWithGas)),
            ]),
            Self::SystemReserveGas => {
                SyscallSignature::gr_fallible([Gas, Ptr(PtrInfo::new_mutable(PtrType::ErrorCode))])
            }
            Self::Random => SyscallSignature::gr_infallible([
                Ptr(PtrInfo::new_immutable(PtrType::Hash(HashType::SubjectId))),
                Ptr(PtrInfo::new_mutable(PtrType::BlockNumberWithHash(
                    HashType::SubjectId,
                ))),
            ]),
            Self::OutOfGas => unimplemented!("Unsupported syscall signature for out_of_gas"),
        }
    }

    pub fn to_wgas(self) -> Option<Self> {
        Some(match self {
            Self::Reply => Self::ReplyWGas,
            Self::ReplyInput => Self::ReplyInputWGas,
            Self::ReplyCommit => Self::ReplyCommitWGas,
            Self::Send => Self::SendWGas,
            Self::SendInput => Self::SendInputWGas,
            Self::SendCommit => Self::SendCommitWGas,
            Self::CreateProgram => Self::CreateProgramWGas,
            _ => return None,
        })
    }

    /// Checks whether the syscall returns error either by writing to input error pointer
    /// or by returning value indicating an error.
    ///
    /// There are only 2 syscalls returning error value: `Alloc`` and `Free`
    pub fn returns_error(self) -> bool {
        let signature = self.signature();
        let has_err_ptr = signature.has_mut_err_pointer();

        let returns_error = match &signature {
            signature @ (SyscallSignature::GrFallible { .. } | SyscallSignature::System { .. }) => {
                if signature.is_fallible() {
                    assert!(
                        has_err_ptr,
                        "error-prone syscall doesn't have mutable err ptr."
                    );
                }

                true
            }
            SyscallSignature::GrInfallible { .. } => {
                assert!(!has_err_ptr, "infallible syscall has mutable err ptr.");

                false
            }
        };

        returns_error
    }

    /// Checks whether the syscall is fallible.
    ///
    /// ### Note:
    /// This differs from `SysCallName::returns_error` as fallible syscalls
    /// are those last param of which is a mutable error pointer.
    pub fn is_fallible(self) -> bool {
        self.signature().is_fallible()
    }
}

/// Syscall param type.
///
/// `Ptr` variant contains additional data about the type this pointer
/// belongs to. See [`PtrInfo`] and [`PtrType`] for more details.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum ParamType {
    Length,              // i32 buffers length
    Ptr(PtrInfo),        // i32 pointer
    Gas,                 // i64 gas amount
    Offset,              // i32 offset in the input buffer (message payload)
    DurationBlockNumber, // i32 duration in blocks
    DelayBlockNumber,    // i32 delay in blocks
    Handler,             // i32 handler number
    Alloc,               // i32 pages to alloc
    Free,                // i32 page number to free
    Version,             // i32 version number of exec settings
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct PtrInfo {
    pub mutable: bool,
    pub ty: PtrType,
}

impl PtrInfo {
    pub fn new_immutable(ty: PtrType) -> PtrInfo {
        PtrInfo { mutable: false, ty }
    }

    pub fn new_mutable(ty: PtrType) -> PtrInfo {
        PtrInfo { mutable: true, ty }
    }
}

/// Hash type.
///
/// Used to distinguish between different hash types in the syscall signatures.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum HashType {
    ActorId,
    CodeId,
    MessageId,
    ReservationId,
    /// This enum variant is used for the `gr_random` syscall.
    SubjectId,
}

/// Pointer type.
///
/// Used to distinguish between different pointer types in the syscall signatures.
/// Basically it responds to different types from `gsys`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum PtrType {
    BlockNumber,
    BlockTimestamp,
    SizedBufferStart { length_param_idx: usize },
    BufferStart,
    Hash(HashType),
    Gas,
    Length,
    Value,

    BlockNumberWithHash(HashType),
    HashWithValue(HashType),
    TwoHashes(HashType, HashType),
    TwoHashesWithValue(HashType, HashType),

    ErrorCode,

    ErrorWithReplyCode,
    ErrorWithSignalCode,
    ErrorWithGas,
    ErrorWithHandle,
    ErrorWithHash(HashType),
    ErrorWithTwoHashes(HashType, HashType),
    ErrorWithBlockNumberAndValue,
}

impl PtrType {
    pub fn is_error(self) -> bool {
        use PtrType::*;

        match self {
            ErrorCode
            | ErrorWithReplyCode
            | ErrorWithSignalCode
            | ErrorWithGas
            | ErrorWithHandle
            | ErrorWithHash(_)
            | ErrorWithTwoHashes(_, _)
            | ErrorWithBlockNumberAndValue => true,
            BlockNumber
            | BlockTimestamp
            | SizedBufferStart { .. }
            | BufferStart
            | Hash(_)
            | Gas
            | Length
            | Value
            | BlockNumberWithHash(_)
            | HashWithValue(_)
            | TwoHashes(_, _)
            | TwoHashesWithValue(_, _) => false,
        }
    }
}

impl From<ParamType> for ValueType {
    fn from(value: ParamType) -> Self {
        match value {
            ParamType::Gas => ValueType::I64,
            _ => ValueType::I32,
        }
    }
}

// TODO: convert to enum SysCallSignature { Gr(param), System { param, results } }
// by that you have a guarantee that gr syscall won't have results until design is rapidly changed.
// it gives more guarantees.

/// Syscall signature.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum SyscallSignature {
    GrFallible {
        params: Vec<ParamType>,
    },
    GrInfallible {
        params: Vec<ParamType>,
    },
    System {
        params: Vec<ParamType>,
        results: Vec<ValueType>,
    },
}

impl SyscallSignature {
    pub fn gr_fallible<const N: usize>(params: [ParamType; N]) -> Self {
        let last_param = params
            .last()
            .expect("fallible syscall has at least one pointer");
        if !matches!(last_param, ParamType::Ptr(PtrInfo { mutable: true, ty }) if ty.is_error()) {
            panic!("Invalid fallible syscall signature.");
        }

        Self::GrFallible {
            params: params.to_vec(),
        }
    }

    pub fn gr_infallible<const N: usize>(params: [ParamType; N]) -> Self {
        if let Some(last_param) = params.last() {
            if matches!(last_param, ParamType::Ptr(PtrInfo { mutable: true, ty }) if ty.is_error())
            {
                panic!("Infallible syscall has mut err ptr.");
            }
        }

        Self::GrInfallible {
            params: params.to_vec(),
        }
    }

    pub fn system<const N: usize, const M: usize>(
        params: [ParamType; N],
        result: [ValueType; M],
    ) -> Self {
        Self::System {
            params: params.to_vec(),
            results: result.to_vec(),
        }
    }

    pub fn func_type(&self) -> FunctionType {
        let (params, results) = match self {
            SyscallSignature::GrFallible { params } => (params, Vec::new()),
            SyscallSignature::GrInfallible { params } => (params, Vec::new()),
            SyscallSignature::System { params, results } => (params, results.clone()),
        };

        FunctionType::new(params.iter().copied().map(Into::into).collect(), results)
    }

    pub fn is_fallible(&self) -> bool {
        matches!(self, SyscallSignature::GrFallible { .. })
    }

    pub fn is_infallible(&self) -> bool {
        matches!(self, SyscallSignature::GrInfallible { .. })
    }

    pub fn is_system(&self) -> bool {
        matches!(self, SyscallSignature::System { .. })
    }

    // TODO remove that by introducing type level guarantees.
    fn has_mut_err_pointer(&self) -> bool {
        let params = match self {
            SyscallSignature::GrFallible { params } => params,
            SyscallSignature::GrInfallible { params } => params,
            SyscallSignature::System { params, .. } => params,
        };

        params.into_iter().any(
            |param| matches!(param, ParamType::Ptr(PtrInfo { mutable: true, ty }) if ty.is_error()),
        )
    }
}

// /// Syscall signature.
// #[derive(Debug, Clone, PartialEq, Eq, Hash)]
// pub struct SyscallSignature {
//     pub params: Vec<ParamType>,
//     pub results: Vec<ValueType>,
// }

// impl SyscallSignature {
//     pub fn gr<const N: usize>(params: [ParamType; N]) -> Self {
//         Self {
//             params: params.to_vec(),
//             results: Default::default(),
//         }
//     }

//     pub fn system<const N: usize, const M: usize>(
//         params: [ParamType; N],
//         results: [ValueType; M],
//     ) -> Self {
//         Self {
//             params: params.to_vec(),
//             results: results.to_vec(),
//         }
//     }

// pub fn func_type(&self) -> FunctionType {
//     FunctionType::new(
//         self.params.iter().copied().map(Into::into).collect(),
//         self.results.clone(),
//     )
// }
// }
