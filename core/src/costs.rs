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

//! Costs module.

use crate::{
    gas::Token,
    pages::{PageU32Size, WasmPage},
};
use core::{fmt::Debug, marker::PhantomData};
use paste::paste;
use scale_info::scale::{Decode, Encode};

/// +_+_+
#[derive(Clone, Copy, PartialEq, Eq, Encode, Decode)]
pub struct CostPer<T> {
    cost: u64,
    _phantom: PhantomData<T>,
}

impl<T> CostPer<T> {
    /// Const constructor
    pub const fn new(cost: u64) -> Self {
        Self {
            cost,
            _phantom: PhantomData,
        }
    }

    /// Cost for one.
    pub const fn one(&self) -> u64 {
        self.cost
    }

    /// Returns another [`CostPer`] with increased `cost` to `other.cost`.
    pub const fn saturating_add(&self, other: Self) -> Self {
        Self::new(self.cost.saturating_add(other.cost))
    }
}

impl<T: Into<u32>> CostPer<T> {
    /// Calculate cost for `num` amount of `T`.
    pub fn calc(&self, num: T) -> u64 {
        self.cost.saturating_mul(Into::<u32>::into(num).into())
    }
}

impl<T> Debug for CostPer<T> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_fmt(format_args!("{}", &self.cost))
    }
}

impl<T> From<u64> for CostPer<T> {
    fn from(cost: u64) -> Self {
        CostPer::new(cost)
    }
}

impl<T> From<CostPer<T>> for u64 {
    fn from(value: CostPer<T>) -> Self {
        value.cost
    }
}

impl<T> Default for CostPer<T> {
    fn default() -> Self {
        CostPer::new(0)
    }
}

/// +_+_+
#[derive(Debug, Default, Clone)]
pub struct Call;

/// +_+_+
#[derive(Debug, Default, Clone, derive_more::From, derive_more::Into)]
pub struct Bytes(u32);

// +_+_+ change naming weights to cost
// Comments
/// Describes the weight for each imported function that a program is allowed to call.
#[derive(Clone, Default)]
pub struct ExtWeights {
    /// Weight of calling `alloc`.
    pub alloc: CostPer<Call>,

    /// Weight per allocated page for `alloc`.
    pub alloc_per_page: CostPer<WasmPage>,

    /// Weight of calling `free`.
    pub free: CostPer<Call>,

    /// Weight of calling `free_range`
    pub free_range: CostPer<Call>,

    /// Weight of calling `free_range` per page
    pub free_range_per_page: CostPer<WasmPage>,

    /// Weight of calling `gr_reserve_gas`.
    pub gr_reserve_gas: CostPer<Call>,

    /// Weight of calling `gr_unreserve_gas`
    pub gr_unreserve_gas: CostPer<Call>,

    /// Weight of calling `gr_system_reserve_gas`
    pub gr_system_reserve_gas: CostPer<Call>,

    /// Weight of calling `gr_gas_available`.
    pub gr_gas_available: CostPer<Call>,

    /// Weight of calling `gr_message_id`.
    pub gr_message_id: CostPer<Call>,

    /// Weight of calling `gr_program_id`.
    pub gr_program_id: CostPer<Call>,

    /// Weight of calling `gr_source`.
    pub gr_source: CostPer<Call>,

    /// Weight of calling `gr_value`.
    pub gr_value: CostPer<Call>,

    /// Weight of calling `gr_value_available`.
    pub gr_value_available: CostPer<Call>,

    /// Weight of calling `gr_size`.
    pub gr_size: CostPer<Call>,

    /// Weight of calling `gr_read`.
    pub gr_read: CostPer<Call>,

    /// Weight per payload byte for `gr_read`.
    pub gr_read_per_byte: CostPer<Bytes>,

    /// Weight of calling `gr_env_vars`.
    pub gr_env_vars: CostPer<Call>,

    /// Weight of calling `gr_block_height`.
    pub gr_block_height: CostPer<Call>,

    /// Weight of calling `gr_block_timestamp`.
    pub gr_block_timestamp: CostPer<Call>,

    /// Weight of calling `gr_random`.
    pub gr_random: CostPer<Call>,

    /// Weight of calling `gr_reply_deposit`.
    pub gr_reply_deposit: CostPer<Call>,

    /// Weight of calling `gr_send`
    pub gr_send: CostPer<Call>,

    /// Weight per bytes for `gr_send`.
    pub gr_send_per_byte: CostPer<Bytes>,

    /// Weight of calling `gr_send_wgas`.
    pub gr_send_wgas: CostPer<Call>,

    /// Weight of calling `gr_send_wgas` per one payload byte.
    pub gr_send_wgas_per_byte: CostPer<Bytes>,

    /// Weight of calling `gr_send_init`.
    pub gr_send_init: CostPer<Call>,

    /// Weight of calling `gr_send_push`.
    pub gr_send_push: CostPer<Call>,

    /// Weight per payload byte by `gr_send_push`.
    pub gr_send_push_per_byte: CostPer<Bytes>,

    /// Weight of calling `gr_send_commit`.
    pub gr_send_commit: CostPer<Call>,

    /// Weight of calling `gr_send_commit_wgas`.
    pub gr_send_commit_wgas: CostPer<Call>,

    /// Weight of calling `gr_reservation_send`.
    pub gr_reservation_send: CostPer<Call>,

    /// Weight of calling `gr_reservation_send` per one payload byte.
    pub gr_reservation_send_per_byte: CostPer<Bytes>,

    /// Weight of calling `gr_reservation_send_commit`.
    pub gr_reservation_send_commit: CostPer<Call>,

    /// Weight of calling `gr_send_init`.
    pub gr_send_input: CostPer<Call>,

    /// Weight of calling `gr_send_init_wgas`.
    pub gr_send_input_wgas: CostPer<Call>,

    /// Weight of calling `gr_send_push_input`.
    pub gr_send_push_input: CostPer<Call>,

    /// Weight per payload byte by `gr_send_push_input`.
    pub gr_send_push_input_per_byte: CostPer<Bytes>,

    /// Weight of calling `gr_reply`.
    pub gr_reply: CostPer<Call>,

    /// Weight of calling `gr_reply` per one payload byte.
    pub gr_reply_per_byte: CostPer<Bytes>,

    /// Weight of calling `gr_reply_wgas`.
    pub gr_reply_wgas: CostPer<Call>,

    /// Weight of calling `gr_reply_wgas` per one payload byte.
    pub gr_reply_wgas_per_byte: CostPer<Bytes>,

    /// Weight of calling `gr_reply_commit`.
    pub gr_reply_commit: CostPer<Call>,

    /// Weight of calling `gr_reply_commit_wgas`.
    pub gr_reply_commit_wgas: CostPer<Call>,

    /// Weight of calling `gr_reservation_reply`.
    pub gr_reservation_reply: CostPer<Call>,

    /// Weight of calling `gr_reservation_reply` per one payload byte.
    pub gr_reservation_reply_per_byte: CostPer<Bytes>,

    /// Weight of calling `gr_reservation_reply_commit`.
    pub gr_reservation_reply_commit: CostPer<Call>,

    /// Weight of calling `gr_reply_push`.
    pub gr_reply_push: CostPer<Call>,

    /// Weight per payload byte by `gr_reply_push`.
    pub gr_reply_push_per_byte: CostPer<Bytes>,

    /// Weight of calling `gr_reply_input`.
    pub gr_reply_input: CostPer<Call>,

    /// Weight of calling `gr_reply_input_wgas`.
    pub gr_reply_input_wgas: CostPer<Call>,

    /// Weight of calling `gr_reply_push_input`.
    pub gr_reply_push_input: CostPer<Call>,

    /// Weight per payload byte by `gr_reply_push_input`.
    pub gr_reply_push_input_per_byte: CostPer<Bytes>,

    /// Weight of calling `gr_reply_to`.
    pub gr_reply_to: CostPer<Call>,

    /// Weight of calling `gr_signal_code`.
    pub gr_signal_code: CostPer<Call>,

    /// Weight of calling `gr_signal_from`.
    pub gr_signal_from: CostPer<Call>,

    /// Weight of calling `gr_debug`.
    pub gr_debug: CostPer<Call>,

    /// Weight per payload byte by `gr_debug`.
    pub gr_debug_per_byte: CostPer<Bytes>,

    /// Weight of calling `gr_reply_code`.
    pub gr_reply_code: CostPer<Call>,

    /// Weight of calling `gr_exit`.
    pub gr_exit: CostPer<Call>,

    /// Weight of calling `gr_leave`.
    pub gr_leave: CostPer<Call>,

    /// Weight of calling `gr_wait`.
    pub gr_wait: CostPer<Call>,

    /// Weight of calling `gr_wait_for`.
    pub gr_wait_for: CostPer<Call>,

    /// Weight of calling `gr_wait_up_to`.
    pub gr_wait_up_to: CostPer<Call>,

    /// Weight of calling `gr_wake`.
    pub gr_wake: CostPer<Call>,

    /// Weight of calling `gr_create_program_wgas`.
    pub gr_create_program: CostPer<Call>,

    /// Weight per payload byte by `gr_create_program_wgas`.
    pub gr_create_program_payload_per_byte: CostPer<Bytes>,

    /// Weight per salt byte by `gr_create_program_wgas`.
    pub gr_create_program_salt_per_byte: CostPer<Bytes>,

    /// Weight of calling `gr_create_program_wgas`.
    pub gr_create_program_wgas: CostPer<Call>,

    /// Weight per payload byte by `gr_create_program_wgas`.
    pub gr_create_program_wgas_payload_per_byte: CostPer<Bytes>,

    /// Weight per salt byte by `gr_create_program_wgas`.
    pub gr_create_program_wgas_salt_per_byte: CostPer<Bytes>,
}

/// Token to consume gas amount.
#[derive(Copy, Clone)]
pub struct RuntimeToken {
    weight: u64,
}

impl From<RuntimeToken> for u64 {
    fn from(value: RuntimeToken) -> Self {
        value.weight
    }
}

impl Token for RuntimeToken {
    fn weight(&self) -> u64 {
        self.weight
    }
}

/// Enumerates syscalls that can be charged by gas meter.
#[derive(Debug, Copy, Clone)]
pub enum CostToken {
    /// Charge zero gas
    Null,
    /// Charge for calling `alloc`, taking into account pages amount.
    Alloc(u32),
    /// Charge for calling `free`.
    Free,
    /// Charge for calling `free_range`
    FreeRange,
    /// Charge for calling `gr_reserve_gas`.
    ReserveGas,
    /// Charge for calling `gr_unreserve_gas`.
    UnreserveGas,
    /// Charge for calling `gr_system_reserve_gas`.
    SystemReserveGas,
    /// Charge for calling `gr_gas_available`.
    GasAvailable,
    /// Charge for calling `gr_message_id`.
    MsgId,
    /// Charge for calling `gr_program_id`.
    ProgramId,
    /// Charge for calling `gr_source`.
    Source,
    /// Charge for calling `gr_value`.
    Value,
    /// Charge for calling `gr_value_available`.
    ValueAvailable,
    /// Charge for calling `gr_size`.
    Size,
    /// Charge for calling `gr_read`.
    Read,
    /// Charge for calling `gr_env_vars`.
    EnvVars,
    /// Charge for calling `gr_block_height`.
    BlockHeight,
    /// Charge for calling `gr_block_timestamp`.
    BlockTimestamp,
    /// Charge for calling `gr_random`.
    Random,
    /// Charge for calling `gr_reply_deposit`.
    ReplyDeposit,
    /// Charge for calling `gr_send`.
    Send(u32),
    /// Charge for calling `gr_send_wgas`.
    SendWGas(u32),
    /// Charge for calling `gr_send_init`.
    SendInit,
    /// Charge for calling `gr_send_push`.
    SendPush(u32),
    /// Charge for calling `gr_send_commit`.
    SendCommit,
    /// Charge for calling `gr_send_commit_wgas`.
    SendCommitWGas,
    /// Charge for calling `gr_reservation_send`.
    ReservationSend(u32),
    /// Charge for calling `gr_reservation_send_commit`.
    ReservationSendCommit,
    /// Charge for calling `gr_send_input`.
    SendInput,
    /// Charge for calling `gr_send_input_wgas`.
    SendInputWGas,
    /// Charge for calling `gr_send_push_input`.
    SendPushInput,
    /// Charge for calling `gr_reply`.
    Reply(u32),
    /// Charge for calling `gr_reply_wgas`.
    ReplyWGas(u32),
    /// Charge for calling `gr_reply_push`.
    ReplyPush(u32),
    /// Charge for calling `gr_reply_commit`.
    ReplyCommit,
    /// Charge for calling `gr_reply_commit_wgas`.
    ReplyCommitWGas,
    /// Charge for calling `gr_reservation_reply`.
    ReservationReply(u32),
    /// Charge for calling `gr_reservation_reply_commit`.
    ReservationReplyCommit,
    /// Charge for calling `gr_reply_input`.
    ReplyInput,
    /// Charge for calling `gr_reply_input_wgas`.
    ReplyInputWGas,
    /// Charge for calling `gr_reply_push_input`.
    ReplyPushInput,
    /// Charge for calling `gr_reply_to`.
    ReplyTo,
    /// Charge for calling `gr_signal_code`.
    SignalCode,
    /// Charge for calling `gr_signal_from`.
    SignalFrom,
    /// Charge for calling `gr_debug`.
    Debug(u32),
    /// Charge for calling `gr_reply_code`.
    ReplyCode,
    /// Charge for calling `gr_exit`.
    Exit,
    /// Charge for calling `gr_leave`.
    Leave,
    /// Charge for calling `gr_wait`.
    Wait,
    /// Charge for calling `gr_wait_for`.
    WaitFor,
    /// Charge for calling `gr_wait_up_to`.
    WaitUpTo,
    /// Charge for calling `gr_wake`.
    Wake,
    /// Charge for calling `gr_create_program`.
    CreateProgram(u32, u32),
    /// Charge for calling `gr_create_program_wgas`.
    CreateProgramWGas(u32, u32),
}

impl CostToken {
    /// Returns a token with a weight given the parameters from `HostFnWeights`.
    pub fn token(&self, s: &ExtWeights) -> RuntimeToken {
        use self::CostToken::*;

        macro_rules! cost_with_weight_per_byte {
            ($name:ident, $len:expr) => {
                paste! {
                    s.$name.one().saturating_add(s.[< $name _per_byte >].calc($len.into()))
                }
            };
        }

        let weight = match *self {
            Null => 0,
            Alloc(pages) => {
                // +_+_+ tmp
                let pages = WasmPage::new(pages).unwrap();
                s.alloc.one().saturating_add(s.alloc_per_page.calc(pages))
            }
            Free => s.free.one(),
            FreeRange => s.free_range.one(),
            ReserveGas => s.gr_reserve_gas.one(),
            UnreserveGas => s.gr_unreserve_gas.one(),
            SystemReserveGas => s.gr_system_reserve_gas.one(),
            GasAvailable => s.gr_gas_available.one(),
            MsgId => s.gr_message_id.one(),
            ProgramId => s.gr_program_id.one(),
            Source => s.gr_source.one(),
            Value => s.gr_value.one(),
            ValueAvailable => s.gr_value_available.one(),
            Size => s.gr_size.one(),
            Read => s.gr_read.one(),
            EnvVars => s.gr_env_vars.one(),
            BlockHeight => s.gr_block_height.one(),
            BlockTimestamp => s.gr_block_timestamp.one(),
            Random => s.gr_random.one(),
            ReplyDeposit => s.gr_reply_deposit.one(),
            Send(len) => cost_with_weight_per_byte!(gr_send, len),
            SendWGas(len) => cost_with_weight_per_byte!(gr_send_wgas, len),
            SendInit => s.gr_send_init.one(),
            SendPush(len) => cost_with_weight_per_byte!(gr_send_push, len),
            SendCommit => s.gr_send_commit.one(),
            SendCommitWGas => s.gr_send_commit_wgas.one(),
            ReservationSend(len) => cost_with_weight_per_byte!(gr_reservation_send, len),
            ReservationSendCommit => s.gr_reservation_send_commit.one(),
            SendInput => s.gr_send_input.one(),
            SendInputWGas => s.gr_send_input_wgas.one(),
            SendPushInput => s.gr_send_push_input.one(),
            Reply(len) => cost_with_weight_per_byte!(gr_reply, len),
            ReplyWGas(len) => cost_with_weight_per_byte!(gr_reply_wgas, len),
            ReplyPush(len) => cost_with_weight_per_byte!(gr_reply_push, len),
            ReplyCommit => s.gr_reply_commit.one(),
            ReplyCommitWGas => s.gr_reply_commit_wgas.one(),
            ReservationReply(len) => cost_with_weight_per_byte!(gr_reservation_reply, len),
            ReservationReplyCommit => s.gr_reservation_reply_commit.one(),
            ReplyInput => s.gr_reply_input.one(),
            ReplyInputWGas => s.gr_reply_input_wgas.one(),
            ReplyPushInput => s.gr_reply_push_input.one(),
            ReplyTo => s.gr_reply_to.one(),
            SignalCode => s.gr_signal_code.one(),
            SignalFrom => s.gr_signal_from.one(),
            Debug(len) => cost_with_weight_per_byte!(gr_debug, len),
            ReplyCode => s.gr_reply_code.one(),
            Exit => s.gr_exit.one(),
            Leave => s.gr_leave.one(),
            Wait => s.gr_wait.one(),
            WaitFor => s.gr_wait_for.one(),
            WaitUpTo => s.gr_wait_up_to.one(),
            Wake => s.gr_wake.one(),
            CreateProgram(payload_len, salt_len) => s
                .gr_create_program
                .one()
                .saturating_add(
                    s.gr_create_program_payload_per_byte
                        .calc(payload_len.into()),
                )
                .saturating_add(s.gr_create_program_salt_per_byte.calc(salt_len.into())),
            CreateProgramWGas(payload_len, salt_len) => s
                .gr_create_program_wgas
                .one()
                .saturating_add(
                    s.gr_create_program_wgas_payload_per_byte
                        .calc(payload_len.into()),
                )
                .saturating_add(s.gr_create_program_wgas_salt_per_byte.calc(salt_len.into())),
        };
        RuntimeToken { weight }
    }
}
