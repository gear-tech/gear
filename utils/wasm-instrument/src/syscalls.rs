// This file is part of Gear.

// Copyright (C) 2022-2024 Gear Technologies Inc.
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

//! Gear syscalls for programs execution signatures.

use crate::parity_wasm::elements::{FunctionType, ValueType};
use alloc::{
    borrow::ToOwned,
    collections::{BTreeMap, BTreeSet},
    string::String,
    vec::Vec,
};
use core::iter;
use enum_iterator::{self, Sequence};
pub use pointers::*;

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
    FreeRange,
    SystemBreak,

    // Miscellaneous
    ReplyDeposit,
    Debug,
    Random,
    ReserveGas,
    UnreserveGas,
    SystemReserveGas,
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
            SyscallName::FreeRange => "free_range",
            SyscallName::GasAvailable => "gr_gas_available",
            SyscallName::Leave => "gr_leave",
            SyscallName::MessageId => "gr_message_id",
            SyscallName::SystemBreak => "gr_system_break",
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
            Self::FreeRange,
            Self::Debug,
            Self::Panic,
            Self::OomPanic,
            Self::BlockHeight,
            Self::BlockTimestamp,
            Self::Exit,
            Self::GasAvailable,
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
            Self::SystemReserveGas,
            Self::EnvVars,
        ]
        .into()
    }

    /// Returns map of all syscall string values to syscall names.
    pub fn instrumentable_map() -> BTreeMap<String, SyscallName> {
        Self::instrumentable()
            .into_iter()
            .map(|syscall| (syscall.to_str().into(), syscall))
            .collect()
    }

    /// Returns signature for syscall by name.
    pub fn signature(self) -> SyscallSignature {
        use RegularParamType::*;

        match self {
            Self::Alloc => SyscallSignature::system(([Alloc], [ValueType::I32])),
            Self::Free => SyscallSignature::system(([Free], [ValueType::I32])),
            Self::FreeRange => SyscallSignature::system(([Free, FreeUpperBound], [ValueType::I32])),
            Self::Debug => SyscallSignature::gr_infallible([
                Ptr::SizedBufferStart {
                    length_param_idx: 1,
                }
                .into(),
                Length,
            ]),
            Self::Panic => SyscallSignature::gr_infallible([
                Ptr::SizedBufferStart {
                    length_param_idx: 1,
                }
                .into(),
                Length,
            ]),
            Self::OomPanic => SyscallSignature::gr_infallible([]),
            Self::BlockHeight => SyscallSignature::gr_infallible([Ptr::MutBlockNumber.into()]),
            Self::BlockTimestamp => {
                SyscallSignature::gr_infallible([Ptr::MutBlockTimestamp.into()])
            }
            Self::Exit => SyscallSignature::gr_infallible([Ptr::Hash(HashType::ActorId).into()]),
            Self::GasAvailable => SyscallSignature::gr_infallible([Ptr::MutGas.into()]),
            Self::ProgramId => {
                SyscallSignature::gr_infallible([Ptr::MutHash(HashType::ActorId).into()])
            }
            Self::Leave => SyscallSignature::gr_infallible([]),
            Self::ValueAvailable => SyscallSignature::gr_infallible([Ptr::MutValue.into()]),
            Self::Wait => SyscallSignature::gr_infallible([]),
            Self::WaitUpTo => SyscallSignature::gr_infallible([DurationBlockNumber]),
            Self::WaitFor => SyscallSignature::gr_infallible([DurationBlockNumber]),
            Self::Wake => SyscallSignature::gr_fallible((
                [Ptr::Hash(HashType::MessageId).into(), DelayBlockNumber],
                ErrPtr::ErrorCode,
            )),
            Self::ReplyCode => SyscallSignature::gr_fallible(ErrPtr::ErrorWithReplyCode),
            Self::SignalCode => SyscallSignature::gr_fallible(ErrPtr::ErrorWithSignalCode),
            Self::MessageId => {
                SyscallSignature::gr_infallible([Ptr::MutHash(HashType::MessageId).into()])
            }
            Self::EnvVars => SyscallSignature::gr_infallible([Version, Ptr::MutBufferStart.into()]),
            Self::Read => SyscallSignature::gr_fallible((
                [
                    Offset,
                    Length,
                    Ptr::MutSizedBufferStart {
                        length_param_idx: 1,
                    }
                    .into(),
                ],
                ErrPtr::ErrorCode,
            )),
            Self::Reply => SyscallSignature::gr_fallible((
                [
                    Ptr::SizedBufferStart {
                        length_param_idx: 1,
                    }
                    .into(),
                    Length,
                    Ptr::Value.into(),
                ],
                ErrPtr::ErrorWithHash(HashType::MessageId),
            )),
            Self::ReplyInput => SyscallSignature::gr_fallible((
                [Offset, Length, Ptr::Value.into()],
                ErrPtr::ErrorWithHash(HashType::MessageId),
            )),
            Self::ReplyWGas => SyscallSignature::gr_fallible((
                [
                    Ptr::SizedBufferStart {
                        length_param_idx: 1,
                    }
                    .into(),
                    Length,
                    Gas,
                    Ptr::Value.into(),
                ],
                ErrPtr::ErrorWithHash(HashType::MessageId),
            )),
            Self::ReplyInputWGas => SyscallSignature::gr_fallible((
                [Offset, Length, Gas, Ptr::Value.into()],
                ErrPtr::ErrorWithHash(HashType::MessageId),
            )),
            Self::ReplyCommit => SyscallSignature::gr_fallible((
                [Ptr::Value.into()],
                ErrPtr::ErrorWithHash(HashType::MessageId),
            )),
            Self::ReplyCommitWGas => SyscallSignature::gr_fallible((
                [Gas, Ptr::Value.into()],
                ErrPtr::ErrorWithHash(HashType::MessageId),
            )),
            Self::ReservationReply => SyscallSignature::gr_fallible((
                [
                    Ptr::HashWithValue(HashType::ReservationId).into(),
                    Ptr::SizedBufferStart {
                        length_param_idx: 2,
                    }
                    .into(),
                    Length,
                ],
                ErrPtr::ErrorWithHash(HashType::MessageId),
            )),
            Self::ReservationReplyCommit => SyscallSignature::gr_fallible((
                [Ptr::HashWithValue(HashType::ReservationId).into()],
                ErrPtr::ErrorWithHash(HashType::MessageId),
            )),
            Self::ReplyPush => SyscallSignature::gr_fallible((
                [
                    Ptr::SizedBufferStart {
                        length_param_idx: 1,
                    }
                    .into(),
                    Length,
                ],
                ErrPtr::ErrorCode,
            )),
            Self::ReplyPushInput => {
                SyscallSignature::gr_fallible(([Offset, Length], ErrPtr::ErrorCode))
            }
            Self::ReplyTo => {
                SyscallSignature::gr_fallible(ErrPtr::ErrorWithHash(HashType::MessageId))
            }
            Self::SignalFrom => {
                SyscallSignature::gr_fallible(ErrPtr::ErrorWithHash(HashType::MessageId))
            }
            Self::Send => SyscallSignature::gr_fallible((
                [
                    Ptr::HashWithValue(HashType::ActorId).into(),
                    Ptr::SizedBufferStart {
                        length_param_idx: 2,
                    }
                    .into(),
                    Length,
                    DelayBlockNumber,
                ],
                ErrPtr::ErrorWithHash(HashType::MessageId),
            )),
            Self::SendInput => SyscallSignature::gr_fallible((
                [
                    Ptr::HashWithValue(HashType::ActorId).into(),
                    Offset,
                    Length,
                    DelayBlockNumber,
                ],
                ErrPtr::ErrorWithHash(HashType::MessageId),
            )),
            Self::SendWGas => SyscallSignature::gr_fallible((
                [
                    Ptr::HashWithValue(HashType::ActorId).into(),
                    Ptr::SizedBufferStart {
                        length_param_idx: 2,
                    }
                    .into(),
                    Length,
                    Gas,
                    DelayBlockNumber,
                ],
                ErrPtr::ErrorWithHash(HashType::MessageId),
            )),
            Self::SendInputWGas => SyscallSignature::gr_fallible((
                [
                    Ptr::HashWithValue(HashType::ActorId).into(),
                    Offset,
                    Length,
                    Gas,
                    DelayBlockNumber,
                ],
                ErrPtr::ErrorWithHash(HashType::MessageId),
            )),
            Self::SendCommit => SyscallSignature::gr_fallible((
                [
                    Handler,
                    Ptr::HashWithValue(HashType::ActorId).into(),
                    DelayBlockNumber,
                ],
                ErrPtr::ErrorWithHash(HashType::MessageId),
            )),
            Self::SendCommitWGas => SyscallSignature::gr_fallible((
                [
                    Handler,
                    Ptr::HashWithValue(HashType::ActorId).into(),
                    Gas,
                    DelayBlockNumber,
                ],
                ErrPtr::ErrorWithHash(HashType::MessageId),
            )),
            Self::SendInit => SyscallSignature::gr_fallible(ErrPtr::ErrorWithHandle),
            Self::SendPush => SyscallSignature::gr_fallible((
                [
                    Handler,
                    Ptr::SizedBufferStart {
                        length_param_idx: 2,
                    }
                    .into(),
                    Length,
                ],
                ErrPtr::ErrorCode,
            )),
            Self::SendPushInput => {
                SyscallSignature::gr_fallible(([Handler, Offset, Length], ErrPtr::ErrorCode))
            }
            Self::ReservationSend => SyscallSignature::gr_fallible((
                [
                    Ptr::TwoHashesWithValue(HashType::ReservationId, HashType::ActorId).into(),
                    Ptr::SizedBufferStart {
                        length_param_idx: 2,
                    }
                    .into(),
                    Length,
                    DelayBlockNumber,
                ],
                ErrPtr::ErrorWithHash(HashType::MessageId),
            )),
            Self::ReservationSendCommit => SyscallSignature::gr_fallible((
                [
                    Handler,
                    Ptr::TwoHashesWithValue(HashType::ReservationId, HashType::ActorId).into(),
                    DelayBlockNumber,
                ],
                ErrPtr::ErrorWithHash(HashType::MessageId),
            )),
            Self::Size => SyscallSignature::gr_infallible([Ptr::MutLength.into()]),
            Self::Source => {
                SyscallSignature::gr_infallible([Ptr::MutHash(HashType::ActorId).into()])
            }
            Self::Value => SyscallSignature::gr_infallible([Ptr::MutValue.into()]),
            Self::CreateProgram => SyscallSignature::gr_fallible((
                [
                    Ptr::HashWithValue(HashType::CodeId).into(),
                    Ptr::SizedBufferStart {
                        length_param_idx: 2,
                    }
                    .into(),
                    Length,
                    Ptr::SizedBufferStart {
                        length_param_idx: 4,
                    }
                    .into(),
                    Length,
                    DelayBlockNumber,
                ],
                ErrPtr::ErrorWithTwoHashes(HashType::MessageId, HashType::ActorId),
            )),
            Self::CreateProgramWGas => SyscallSignature::gr_fallible((
                [
                    Ptr::HashWithValue(HashType::CodeId).into(),
                    Ptr::SizedBufferStart {
                        length_param_idx: 2,
                    }
                    .into(),
                    Length,
                    Ptr::SizedBufferStart {
                        length_param_idx: 4,
                    }
                    .into(),
                    Length,
                    Gas,
                    DelayBlockNumber,
                ],
                ErrPtr::ErrorWithTwoHashes(HashType::MessageId, HashType::ActorId),
            )),
            Self::ReplyDeposit => SyscallSignature::gr_fallible((
                [Ptr::Hash(HashType::MessageId).into(), Gas],
                ErrPtr::ErrorCode,
            )),
            Self::ReserveGas => SyscallSignature::gr_fallible((
                [Gas, DurationBlockNumber],
                ErrPtr::ErrorWithHash(HashType::ReservationId),
            )),
            Self::UnreserveGas => SyscallSignature::gr_fallible((
                [Ptr::Hash(HashType::ReservationId).into()],
                ErrPtr::ErrorWithGas,
            )),
            Self::SystemReserveGas => SyscallSignature::gr_fallible(([Gas], ErrPtr::ErrorCode)),
            Self::Random => SyscallSignature::gr_infallible([
                Ptr::Hash(HashType::SubjectId).into(),
                Ptr::MutBlockNumberWithHash(HashType::SubjectId).into(),
            ]),
            Self::SystemBreak => unimplemented!("Unsupported syscall signature for system_break"),
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
    /// There are only 3 syscalls returning error value: `Alloc`, `Free` & `FreeRange`.
    pub fn returns_error(self) -> bool {
        let signature = self.signature();

        match &signature {
            SyscallSignature::Fallible(_) | SyscallSignature::System(_) => true,
            SyscallSignature::Infallible(_) => false,
        }
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
    Regular(RegularParamType),
    Error(ErrPtr),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum RegularParamType {
    Length,              // i32 buffers length
    Pointer(Ptr),        // i32 non-error pointer
    Gas,                 // i64 gas amount
    Offset,              // i32 offset in the input buffer (message payload)
    DurationBlockNumber, // i32 duration in blocks
    DelayBlockNumber,    // i32 delay in blocks
    Handler,             // i32 handler number
    Alloc,               // i32 pages to alloc
    Free,                // i32 page number to free
    FreeUpperBound,      // i32 free upper bound for use with free_range
    Version,             // i32 version number of exec settings
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

impl From<ParamType> for ValueType {
    fn from(value: ParamType) -> Self {
        use RegularParamType::*;

        match value {
            ParamType::Regular(regular_ptr) => match regular_ptr {
                Length | Pointer(_) | Offset | DurationBlockNumber | DelayBlockNumber | Handler
                | Alloc | Free | FreeUpperBound | Version => ValueType::I32,
                Gas => ValueType::I64,
            },
            ParamType::Error(_) => ValueType::I32,
        }
    }
}

/// Syscall signature.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum SyscallSignature {
    Fallible(FallibleSyscallSignature),
    Infallible(InfallibleSyscallSignature),
    System(SystemSyscallSignature),
}

impl SyscallSignature {
    pub fn gr_fallible(fallible: impl Into<FallibleSyscallSignature>) -> Self {
        Self::Fallible(fallible.into())
    }

    pub fn gr_infallible(infallible: impl Into<InfallibleSyscallSignature>) -> Self {
        Self::Infallible(infallible.into())
    }

    pub fn system(system: impl Into<SystemSyscallSignature>) -> Self {
        Self::System(system.into())
    }

    pub fn params(&self) -> &[ParamType] {
        match self {
            SyscallSignature::Fallible(fallible) => &fallible.0,
            SyscallSignature::Infallible(infallible) => &infallible.0,
            SyscallSignature::System(system) => &system.params,
        }
    }

    pub fn results(&self) -> Option<&[ValueType]> {
        match self {
            SyscallSignature::Fallible(_) | SyscallSignature::Infallible(_) => None,
            SyscallSignature::System(system) => Some(&system.results),
        }
    }

    pub fn func_type(&self) -> FunctionType {
        let (params, results) = match self {
            SyscallSignature::Fallible(fallible) => (fallible.params(), Vec::new()),
            SyscallSignature::Infallible(infallible) => (infallible.params(), Vec::new()),
            SyscallSignature::System(system) => (system.params(), system.results().to_owned()),
        };

        FunctionType::new(params.iter().copied().map(Into::into).collect(), results)
    }

    pub fn is_fallible(&self) -> bool {
        matches!(self, SyscallSignature::Fallible(_))
    }

    pub fn is_infallible(&self) -> bool {
        matches!(self, SyscallSignature::Infallible(_))
    }

    pub fn is_system(&self) -> bool {
        matches!(self, SyscallSignature::System(_))
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct FallibleSyscallSignature(Vec<ParamType>);

impl FallibleSyscallSignature {
    pub fn new<const N: usize>(params: [RegularParamType; N], err_ptr: ErrPtr) -> Self {
        let params = params
            .into_iter()
            .map(ParamType::Regular)
            .chain(iter::once(err_ptr.into()))
            .collect();

        FallibleSyscallSignature(params)
    }

    pub fn params(&self) -> &[ParamType] {
        &self.0
    }
}

impl<const N: usize> From<([RegularParamType; N], ErrPtr)> for FallibleSyscallSignature {
    fn from((params, err_ptr): ([RegularParamType; N], ErrPtr)) -> Self {
        FallibleSyscallSignature::new(params, err_ptr)
    }
}

impl From<ErrPtr> for FallibleSyscallSignature {
    fn from(err_ptr: ErrPtr) -> Self {
        FallibleSyscallSignature::new([], err_ptr)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct InfallibleSyscallSignature(Vec<ParamType>);

impl InfallibleSyscallSignature {
    pub fn new<const N: usize>(params: [RegularParamType; N]) -> Self {
        InfallibleSyscallSignature(params.into_iter().map(ParamType::Regular).collect())
    }

    pub fn params(&self) -> &[ParamType] {
        &self.0
    }
}

impl<const N: usize> From<[RegularParamType; N]> for InfallibleSyscallSignature {
    fn from(params: [RegularParamType; N]) -> Self {
        InfallibleSyscallSignature::new(params)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct SystemSyscallSignature {
    params: Vec<ParamType>,
    results: Vec<ValueType>,
}

impl SystemSyscallSignature {
    pub fn new<const N: usize, const M: usize>(
        params: [RegularParamType; N],
        results: [ValueType; M],
    ) -> Self {
        SystemSyscallSignature {
            params: params.into_iter().map(ParamType::Regular).collect(),
            results: results.to_vec(),
        }
    }

    pub fn params(&self) -> &[ParamType] {
        &self.params
    }

    pub fn results(&self) -> &[ValueType] {
        &self.results
    }
}

impl<const N: usize, const M: usize> From<([RegularParamType; N], [ValueType; M])>
    for SystemSyscallSignature
{
    fn from((params, results): ([RegularParamType; N], [ValueType; M])) -> Self {
        SystemSyscallSignature::new(params, results)
    }
}

// TODO: issue write macros
mod pointers {
    use super::{HashType, ParamType, RegularParamType};

    #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
    pub enum Ptr {
        // Const ptrs.
        SizedBufferStart { length_param_idx: usize },
        Hash(HashType),
        Value,
        HashWithValue(HashType),
        TwoHashes(HashType, HashType),
        TwoHashesWithValue(HashType, HashType),
        // Mutable ptrs.
        MutBlockNumber,
        MutBlockTimestamp,
        MutSizedBufferStart { length_param_idx: usize },
        MutBufferStart,
        MutHash(HashType),
        MutGas,
        MutLength,
        MutValue,
        MutBlockNumberWithHash(HashType),
    }

    impl Ptr {
        pub fn is_mutable(self) -> bool {
            use Ptr::*;

            match self {
                SizedBufferStart { .. }
                | Hash(_)
                | Value
                | HashWithValue(_)
                | TwoHashes(_, _)
                | TwoHashesWithValue(_, _) => false,
                MutBlockNumber
                | MutBlockTimestamp
                | MutSizedBufferStart { .. }
                | MutBufferStart
                | MutHash(_)
                | MutGas
                | MutLength
                | MutValue
                | MutBlockNumberWithHash(_) => true,
            }
        }
    }

    impl From<Ptr> for RegularParamType {
        fn from(ptr: Ptr) -> RegularParamType {
            RegularParamType::Pointer(ptr)
        }
    }

    #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
    pub enum ErrPtr {
        ErrorCode,
        ErrorWithReplyCode,
        ErrorWithSignalCode,
        ErrorWithGas,
        ErrorWithHandle,
        ErrorWithHash(HashType),
        ErrorWithTwoHashes(HashType, HashType),
    }

    impl From<ErrPtr> for ParamType {
        fn from(err_ptr: ErrPtr) -> ParamType {
            ParamType::Error(err_ptr)
        }
    }
}
