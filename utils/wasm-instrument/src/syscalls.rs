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

/// All available sys calls.
///
/// The type is mainly used to prevent from skipping sys-call integration test for
/// a newly introduced sys-call or from typo in sys-call name.
#[derive(Debug, Clone, Copy, Ord, PartialOrd, Eq, PartialEq, Sequence, Hash)]
pub enum SysCallName {
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

impl SysCallName {
    pub fn to_str(&self) -> &'static str {
        match self {
            SysCallName::Alloc => "alloc",
            SysCallName::BlockHeight => "gr_block_height",
            SysCallName::BlockTimestamp => "gr_block_timestamp",
            SysCallName::CreateProgram => "gr_create_program",
            SysCallName::CreateProgramWGas => "gr_create_program_wgas",
            SysCallName::ReplyDeposit => "gr_reply_deposit",
            SysCallName::Debug => "gr_debug",
            SysCallName::Panic => "gr_panic",
            SysCallName::OomPanic => "gr_oom_panic",
            SysCallName::Exit => "gr_exit",
            SysCallName::Free => "free",
            SysCallName::GasAvailable => "gr_gas_available",
            SysCallName::Leave => "gr_leave",
            SysCallName::MessageId => "gr_message_id",
            SysCallName::OutOfGas => "gr_out_of_gas",
            SysCallName::PayProgramRent => "gr_pay_program_rent",
            SysCallName::ProgramId => "gr_program_id",
            SysCallName::Random => "gr_random",
            SysCallName::Read => "gr_read",
            SysCallName::Reply => "gr_reply",
            SysCallName::ReplyCommit => "gr_reply_commit",
            SysCallName::ReplyCommitWGas => "gr_reply_commit_wgas",
            SysCallName::ReplyPush => "gr_reply_push",
            SysCallName::ReplyTo => "gr_reply_to",
            SysCallName::SignalFrom => "gr_signal_from",
            SysCallName::ReplyWGas => "gr_reply_wgas",
            SysCallName::ReplyInput => "gr_reply_input",
            SysCallName::ReplyPushInput => "gr_reply_push_input",
            SysCallName::ReplyInputWGas => "gr_reply_input_wgas",
            SysCallName::ReservationReply => "gr_reservation_reply",
            SysCallName::ReservationReplyCommit => "gr_reservation_reply_commit",
            SysCallName::ReservationSend => "gr_reservation_send",
            SysCallName::ReservationSendCommit => "gr_reservation_send_commit",
            SysCallName::ReserveGas => "gr_reserve_gas",
            SysCallName::Send => "gr_send",
            SysCallName::SendCommit => "gr_send_commit",
            SysCallName::SendCommitWGas => "gr_send_commit_wgas",
            SysCallName::SendInit => "gr_send_init",
            SysCallName::SendPush => "gr_send_push",
            SysCallName::SendWGas => "gr_send_wgas",
            SysCallName::SendInput => "gr_send_input",
            SysCallName::SendPushInput => "gr_send_push_input",
            SysCallName::SendInputWGas => "gr_send_input_wgas",
            SysCallName::Size => "gr_size",
            SysCallName::Source => "gr_source",
            SysCallName::ReplyCode => "gr_reply_code",
            SysCallName::SignalCode => "gr_signal_code",
            SysCallName::SystemReserveGas => "gr_system_reserve_gas",
            SysCallName::UnreserveGas => "gr_unreserve_gas",
            SysCallName::Value => "gr_value",
            SysCallName::ValueAvailable => "gr_value_available",
            SysCallName::Wait => "gr_wait",
            SysCallName::WaitFor => "gr_wait_for",
            SysCallName::WaitUpTo => "gr_wait_up_to",
            SysCallName::Wake => "gr_wake",
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
    pub fn signature(self) -> SysCallSignature {
        use ParamType::*;
        use ValueType::I32;
        match self {
            Self::Alloc => SysCallSignature::system([Alloc], [I32]),
            Self::Free => SysCallSignature::system([Free], [I32]),
            Self::Debug => SysCallSignature::gr([
                Ptr(PtrInfo::new_immutable(PtrType::BufferStart {
                    length_param_idx: 1,
                })),
                Size,
            ]),
            Self::Panic => SysCallSignature::gr([
                Ptr(PtrInfo::new_immutable(PtrType::BufferStart {
                    length_param_idx: 1,
                })),
                Size,
            ]),
            Self::OomPanic => SysCallSignature::gr([]),
            Self::BlockHeight => {
                SysCallSignature::gr([Ptr(PtrInfo::new_mutable(PtrType::BlockNumber))])
            }
            Self::BlockTimestamp => {
                SysCallSignature::gr([Ptr(PtrInfo::new_mutable(PtrType::BlockTimestamp))])
            }
            Self::Exit => SysCallSignature::gr([Ptr(PtrInfo::new_immutable(PtrType::Hash(
                HashType::ActorId,
            )))]),
            Self::GasAvailable => SysCallSignature::gr([Ptr(PtrInfo::new_mutable(PtrType::Gas))]),
            Self::PayProgramRent => SysCallSignature::gr([
                Ptr(PtrInfo::new_immutable(PtrType::HashWithValue(
                    HashType::ActorId,
                ))),
                Ptr(PtrInfo::new_mutable(PtrType::ErrorWithBlockNumberAndValue)),
            ]),
            Self::ProgramId => {
                SysCallSignature::gr([Ptr(PtrInfo::new_mutable(PtrType::Hash(HashType::ActorId)))])
            }
            Self::Leave => SysCallSignature::gr([]),
            Self::ValueAvailable => {
                SysCallSignature::gr([Ptr(PtrInfo::new_mutable(PtrType::Value))])
            }
            Self::Wait => SysCallSignature::gr([]),
            Self::WaitUpTo => SysCallSignature::gr([Duration]),
            Self::WaitFor => SysCallSignature::gr([Duration]),
            Self::Wake => SysCallSignature::gr([
                Ptr(PtrInfo::new_immutable(PtrType::Hash(HashType::MessageId))),
                Delay,
                Ptr(PtrInfo::new_mutable(PtrType::ErrorCode)),
            ]),
            Self::ReplyCode => {
                SysCallSignature::gr([Ptr(PtrInfo::new_mutable(PtrType::ErrorWithReplyCode))])
            }
            Self::SignalCode => {
                SysCallSignature::gr([Ptr(PtrInfo::new_mutable(PtrType::ErrorWithSignalCode))])
            }
            Self::MessageId => SysCallSignature::gr([Ptr(PtrInfo::new_mutable(PtrType::Hash(
                HashType::MessageId,
            )))]),
            Self::Read => SysCallSignature::gr([
                MessagePosition,
                Size,
                Ptr(PtrInfo::new_mutable(PtrType::BufferStart {
                    length_param_idx: 1,
                })),
                Ptr(PtrInfo::new_mutable(PtrType::ErrorCode)),
            ]),
            Self::Reply => SysCallSignature::gr([
                Ptr(PtrInfo::new_immutable(PtrType::BufferStart {
                    length_param_idx: 1,
                })),
                Size,
                Ptr(PtrInfo::new_immutable(PtrType::Value)),
                Ptr(PtrInfo::new_mutable(PtrType::ErrorWithHash(
                    HashType::MessageId,
                ))),
            ]),
            Self::ReplyInput => SysCallSignature::gr([
                Size,
                Size,
                Ptr(PtrInfo::new_immutable(PtrType::Value)),
                Ptr(PtrInfo::new_mutable(PtrType::ErrorWithHash(
                    HashType::MessageId,
                ))),
            ]),
            Self::ReplyWGas => SysCallSignature::gr([
                Ptr(PtrInfo::new_immutable(PtrType::BufferStart {
                    length_param_idx: 1,
                })),
                Size,
                Gas,
                Ptr(PtrInfo::new_immutable(PtrType::Value)),
                Ptr(PtrInfo::new_mutable(PtrType::ErrorWithHash(
                    HashType::MessageId,
                ))),
            ]),
            Self::ReplyInputWGas => SysCallSignature::gr([
                Size,
                Size,
                Gas,
                Ptr(PtrInfo::new_immutable(PtrType::Value)),
                Ptr(PtrInfo::new_mutable(PtrType::ErrorWithHash(
                    HashType::MessageId,
                ))),
            ]),
            Self::ReplyCommit => SysCallSignature::gr([
                Ptr(PtrInfo::new_immutable(PtrType::Value)),
                Ptr(PtrInfo::new_mutable(PtrType::ErrorWithHash(
                    HashType::MessageId,
                ))),
            ]),
            Self::ReplyCommitWGas => SysCallSignature::gr([
                Gas,
                Ptr(PtrInfo::new_immutable(PtrType::Value)),
                Ptr(PtrInfo::new_mutable(PtrType::ErrorWithHash(
                    HashType::MessageId,
                ))),
            ]),
            Self::ReservationReply => SysCallSignature::gr([
                Ptr(PtrInfo::new_immutable(PtrType::HashWithValue(
                    HashType::ReservationId,
                ))),
                Ptr(PtrInfo::new_immutable(PtrType::BufferStart {
                    length_param_idx: 2,
                })),
                Size,
                Ptr(PtrInfo::new_mutable(PtrType::ErrorWithHash(
                    HashType::MessageId,
                ))),
            ]),
            Self::ReservationReplyCommit => SysCallSignature::gr([
                Ptr(PtrInfo::new_immutable(PtrType::HashWithValue(
                    HashType::ReservationId,
                ))),
                Ptr(PtrInfo::new_mutable(PtrType::ErrorWithHash(
                    HashType::MessageId,
                ))),
            ]),
            Self::ReplyPush => SysCallSignature::gr([
                Ptr(PtrInfo::new_immutable(PtrType::BufferStart {
                    length_param_idx: 1,
                })),
                Size,
                Ptr(PtrInfo::new_mutable(PtrType::ErrorCode)),
            ]),
            Self::ReplyPushInput => {
                SysCallSignature::gr([Size, Size, Ptr(PtrInfo::new_mutable(PtrType::ErrorCode))])
            }
            Self::ReplyTo => SysCallSignature::gr([Ptr(PtrInfo::new_mutable(
                PtrType::ErrorWithHash(HashType::MessageId),
            ))]),
            Self::SignalFrom => SysCallSignature::gr([Ptr(PtrInfo::new_mutable(
                PtrType::ErrorWithHash(HashType::MessageId),
            ))]),
            Self::Send => SysCallSignature::gr([
                Ptr(PtrInfo::new_immutable(PtrType::HashWithValue(
                    HashType::ActorId,
                ))),
                Ptr(PtrInfo::new_immutable(PtrType::BufferStart {
                    length_param_idx: 2,
                })),
                Size,
                Delay,
                Ptr(PtrInfo::new_mutable(PtrType::ErrorWithHash(
                    HashType::MessageId,
                ))),
            ]),
            Self::SendInput => SysCallSignature::gr([
                Ptr(PtrInfo::new_immutable(PtrType::HashWithValue(
                    HashType::ActorId,
                ))),
                Size,
                Size,
                Delay,
                Ptr(PtrInfo::new_mutable(PtrType::ErrorWithHash(
                    HashType::MessageId,
                ))),
            ]),
            Self::SendWGas => SysCallSignature::gr([
                Ptr(PtrInfo::new_immutable(PtrType::HashWithValue(
                    HashType::ActorId,
                ))),
                Ptr(PtrInfo::new_immutable(PtrType::BufferStart {
                    length_param_idx: 2,
                })),
                Size,
                Gas,
                Delay,
                Ptr(PtrInfo::new_mutable(PtrType::ErrorWithHash(
                    HashType::MessageId,
                ))),
            ]),
            Self::SendInputWGas => SysCallSignature::gr([
                Ptr(PtrInfo::new_immutable(PtrType::HashWithValue(
                    HashType::ActorId,
                ))),
                Size,
                Size,
                Gas,
                Delay,
                Ptr(PtrInfo::new_mutable(PtrType::ErrorWithHash(
                    HashType::MessageId,
                ))),
            ]),
            Self::SendCommit => SysCallSignature::gr([
                Handler,
                Ptr(PtrInfo::new_immutable(PtrType::HashWithValue(
                    HashType::ActorId,
                ))),
                Delay,
                Ptr(PtrInfo::new_mutable(PtrType::ErrorWithHash(
                    HashType::MessageId,
                ))),
            ]),
            Self::SendCommitWGas => SysCallSignature::gr([
                Handler,
                Ptr(PtrInfo::new_immutable(PtrType::HashWithValue(
                    HashType::ActorId,
                ))),
                Gas,
                Delay,
                Ptr(PtrInfo::new_mutable(PtrType::ErrorWithHash(
                    HashType::MessageId,
                ))),
            ]),
            Self::SendInit => {
                SysCallSignature::gr([Ptr(PtrInfo::new_mutable(PtrType::ErrorWithHandle))])
            }
            Self::SendPush => SysCallSignature::gr([
                Handler,
                Ptr(PtrInfo::new_immutable(PtrType::BufferStart {
                    length_param_idx: 2,
                })),
                Size,
                Ptr(PtrInfo::new_mutable(PtrType::ErrorCode)),
            ]),
            Self::SendPushInput => SysCallSignature::gr([
                Handler,
                Size,
                Size,
                Ptr(PtrInfo::new_mutable(PtrType::ErrorCode)),
            ]),
            Self::ReservationSend => SysCallSignature::gr([
                Ptr(PtrInfo::new_immutable(PtrType::TwoHashesWithValue(
                    HashType::ReservationId,
                    HashType::ActorId,
                ))),
                Ptr(PtrInfo::new_immutable(PtrType::BufferStart {
                    length_param_idx: 2,
                })),
                Size,
                Delay,
                Ptr(PtrInfo::new_mutable(PtrType::ErrorWithHash(
                    HashType::MessageId,
                ))),
            ]),
            Self::ReservationSendCommit => SysCallSignature::gr([
                Handler,
                Ptr(PtrInfo::new_immutable(PtrType::TwoHashesWithValue(
                    HashType::ReservationId,
                    HashType::ActorId,
                ))),
                Delay,
                Ptr(PtrInfo::new_mutable(PtrType::ErrorWithHash(
                    HashType::MessageId,
                ))),
            ]),
            Self::Size => SysCallSignature::gr([Ptr(PtrInfo::new_mutable(PtrType::Length))]),
            Self::Source => {
                SysCallSignature::gr([Ptr(PtrInfo::new_mutable(PtrType::Hash(HashType::ActorId)))])
            }
            Self::Value => SysCallSignature::gr([Ptr(PtrInfo::new_mutable(PtrType::Value))]),
            Self::CreateProgram => SysCallSignature::gr([
                Ptr(PtrInfo::new_immutable(PtrType::HashWithValue(
                    HashType::CodeId,
                ))),
                Ptr(PtrInfo::new_immutable(PtrType::BufferStart {
                    length_param_idx: 2,
                })),
                Size,
                Ptr(PtrInfo::new_immutable(PtrType::BufferStart {
                    length_param_idx: 4,
                })),
                Size,
                Delay,
                Ptr(PtrInfo::new_mutable(PtrType::ErrorWithTwoHashes(
                    HashType::MessageId,
                    HashType::ActorId,
                ))),
            ]),
            Self::CreateProgramWGas => SysCallSignature::gr([
                Ptr(PtrInfo::new_immutable(PtrType::HashWithValue(
                    HashType::CodeId,
                ))),
                Ptr(PtrInfo::new_immutable(PtrType::BufferStart {
                    length_param_idx: 2,
                })),
                Size,
                Ptr(PtrInfo::new_immutable(PtrType::BufferStart {
                    length_param_idx: 4,
                })),
                Size,
                Gas,
                Delay,
                Ptr(PtrInfo::new_mutable(PtrType::ErrorWithTwoHashes(
                    HashType::MessageId,
                    HashType::ActorId,
                ))),
            ]),
            Self::ReplyDeposit => SysCallSignature::gr([
                Ptr(PtrInfo::new_immutable(PtrType::Hash(HashType::MessageId))),
                Gas,
                Ptr(PtrInfo::new_mutable(PtrType::ErrorCode)),
            ]),
            Self::ReserveGas => SysCallSignature::gr([
                Gas,
                Duration,
                Ptr(PtrInfo::new_mutable(PtrType::ErrorWithHash(
                    HashType::ReservationId,
                ))),
            ]),
            Self::UnreserveGas => SysCallSignature::gr([
                Ptr(PtrInfo::new_immutable(PtrType::Hash(
                    HashType::ReservationId,
                ))),
                Ptr(PtrInfo::new_mutable(PtrType::ErrorWithGas)),
            ]),
            Self::SystemReserveGas => {
                SysCallSignature::gr([Gas, Ptr(PtrInfo::new_mutable(PtrType::ErrorCode))])
            }
            Self::Random => SysCallSignature::gr([
                Ptr(PtrInfo::new_immutable(PtrType::Hash(HashType::SubjectId))),
                Ptr(PtrInfo::new_mutable(PtrType::BlockNumberWithHash(
                    HashType::SubjectId,
                ))),
            ]),
            other => panic!("Unknown syscall: '{:?}'", other),
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
}

/// Syscall param type.
///
/// `Ptr` variant contains additional data about the type this pointer
/// belongs to. See [`PtrInfo`] and [`PtrType`] for more details.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum ParamType {
    Size,            // i32 buffers size in memory
    Ptr(PtrInfo),    // i32 pointer
    Gas,             // i64 gas amount
    MessagePosition, // i32 message position
    Duration,        // i32 duration in blocks
    Delay,           // i32 delay in blocks
    Handler,         // i32 handler number
    Alloc,           // i32 alloc pages
    Free,            // i32 free page
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
    BufferStart { length_param_idx: usize },
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
            | BufferStart { .. }
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

/// Syscall signature.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct SysCallSignature {
    pub params: Vec<ParamType>,
    pub results: Vec<ValueType>,
}

impl SysCallSignature {
    pub fn gr<const N: usize>(params: [ParamType; N]) -> Self {
        Self {
            params: params.to_vec(),
            results: Default::default(),
        }
    }

    pub fn system<const N: usize, const M: usize>(
        params: [ParamType; N],
        results: [ValueType; M],
    ) -> Self {
        Self {
            params: params.to_vec(),
            results: results.to_vec(),
        }
    }

    pub fn func_type(&self) -> FunctionType {
        FunctionType::new(
            self.params.iter().copied().map(Into::into).collect(),
            self.results.clone(),
        )
    }
}
