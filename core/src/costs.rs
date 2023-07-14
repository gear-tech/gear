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

//! Costs module.

use crate::{gas::Token, pages::PageU32Size};
use core::{fmt::Debug, marker::PhantomData};
use paste::paste;
use scale_info::scale::{Decode, Encode};

/// Cost per one memory page.
#[derive(Clone, Copy, PartialEq, Eq, Encode, Decode)]
pub struct CostPerPage<P: PageU32Size> {
    cost: u64,
    _phantom: PhantomData<P>,
}

impl<P: PageU32Size> CostPerPage<P> {
    /// Const constructor
    pub const fn new(cost: u64) -> Self {
        Self {
            cost,
            _phantom: PhantomData,
        }
    }

    /// Calculate cost for `pages`.
    pub fn calc(&self, pages: P) -> u64 {
        self.cost.saturating_mul(pages.raw() as u64)
    }

    /// Cost for one page.
    pub fn one(&self) -> u64 {
        self.cost
    }

    /// Returns another [CostPerPage] with increased `cost` to `other.cost`.
    pub fn saturating_add(&self, other: Self) -> Self {
        self.cost.saturating_add(other.cost).into()
    }
}

impl<P: PageU32Size> Debug for CostPerPage<P> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_fmt(format_args!("{}", &self.cost))
    }
}

impl<P: PageU32Size> From<u64> for CostPerPage<P> {
    fn from(cost: u64) -> Self {
        CostPerPage {
            cost,
            _phantom: PhantomData,
        }
    }
}

impl<P: PageU32Size> From<CostPerPage<P>> for u64 {
    fn from(value: CostPerPage<P>) -> Self {
        value.cost
    }
}

impl<P: PageU32Size> Default for CostPerPage<P> {
    fn default() -> Self {
        Self {
            cost: 0,
            _phantom: PhantomData,
        }
    }
}

/// Describes the weight for each imported function that a program is allowed to call.
#[derive(Clone, Encode, Decode, PartialEq, Eq, Default)]
pub struct HostFnWeights {
    /// Weight of calling `alloc`.
    pub alloc: u64,

    /// Weight per allocated page for `alloc`.
    pub alloc_per_page: u64,

    /// Weight of calling `alloc`.
    pub free: u64,

    /// Weight of calling `gr_reserve_gas`.
    pub gr_reserve_gas: u64,

    /// Weight of calling `gr_unreserve_gas`
    pub gr_unreserve_gas: u64,

    /// Weight of calling `gr_system_reserve_gas`
    pub gr_system_reserve_gas: u64,

    /// Weight of calling `gr_gas_available`.
    pub gr_gas_available: u64,

    /// Weight of calling `gr_message_id`.
    pub gr_message_id: u64,

    /// Weight of calling `gr_pay_program_rent`.
    pub gr_pay_program_rent: u64,

    /// Weight of calling `gr_program_id`.
    pub gr_program_id: u64,

    /// Weight of calling `gr_source`.
    pub gr_source: u64,

    /// Weight of calling `gr_value`.
    pub gr_value: u64,

    /// Weight of calling `gr_value_available`.
    pub gr_value_available: u64,

    /// Weight of calling `gr_size`.
    pub gr_size: u64,

    /// Weight of calling `gr_read`.
    pub gr_read: u64,

    /// Weight per payload byte by `gr_read`.
    pub gr_read_per_byte: u64,

    /// Weight of calling `gr_block_height`.
    pub gr_block_height: u64,

    /// Weight of calling `gr_block_timestamp`.
    pub gr_block_timestamp: u64,

    /// Weight of calling `gr_random`.
    pub gr_random: u64,

    /// Weight of calling `gr_reply_deposit`.
    pub gr_reply_deposit: u64,

    /// Weight of calling `gr_send`
    pub gr_send: u64,

    /// Weight of calling `gr_send` per one payload byte.
    pub gr_send_per_byte: u64,

    /// Weight of calling `gr_send_wgas`.
    pub gr_send_wgas: u64,

    /// Weight of calling `gr_send_wgas` per one payload byte.
    pub gr_send_wgas_per_byte: u64,

    /// Weight of calling `gr_send_init`.
    pub gr_send_init: u64,

    /// Weight of calling `gr_send_push`.
    pub gr_send_push: u64,

    /// Weight per payload byte by `gr_send_push`.
    pub gr_send_push_per_byte: u64,

    /// Weight of calling `gr_send_commit`.
    pub gr_send_commit: u64,

    /// Weight of calling `gr_send_commit_wgas`.
    pub gr_send_commit_wgas: u64,

    /// Weight of calling `gr_reservation_send`.
    pub gr_reservation_send: u64,

    /// Weight of calling `gr_reservation_send` per one payload byte.
    pub gr_reservation_send_per_byte: u64,

    /// Weight of calling `gr_reservation_send_commit`.
    pub gr_reservation_send_commit: u64,

    /// Weight of calling `gr_send_init`.
    pub gr_send_input: u64,

    /// Weight of calling `gr_send_init_wgas`.
    pub gr_send_input_wgas: u64,

    /// Weight of calling `gr_send_push_input`.
    pub gr_send_push_input: u64,

    /// Weight per payload byte by `gr_send_push_input`.
    pub gr_send_push_input_per_byte: u64,

    /// Weight of calling `gr_reply`.
    pub gr_reply: u64,

    /// Weight of calling `gr_reply` per one payload byte.
    pub gr_reply_per_byte: u64,

    /// Weight of calling `gr_reply_wgas`.
    pub gr_reply_wgas: u64,

    /// Weight of calling `gr_reply_wgas` per one payload byte.
    pub gr_reply_wgas_per_byte: u64,

    /// Weight of calling `gr_reply_commit`.
    pub gr_reply_commit: u64,

    /// Weight of calling `gr_reply_commit_wgas`.
    pub gr_reply_commit_wgas: u64,

    /// Weight of calling `gr_reservation_reply`.
    pub gr_reservation_reply: u64,

    /// Weight of calling `gr_reservation_reply` per one payload byte.
    pub gr_reservation_reply_per_byte: u64,

    /// Weight of calling `gr_reservation_reply_commit`.
    pub gr_reservation_reply_commit: u64,

    /// Weight of calling `gr_reply_push`.
    pub gr_reply_push: u64,

    /// Weight per payload byte by `gr_reply_push`.
    pub gr_reply_push_per_byte: u64,

    /// Weight of calling `gr_reply_input`.
    pub gr_reply_input: u64,

    /// Weight of calling `gr_reply_input_wgas`.
    pub gr_reply_input_wgas: u64,

    /// Weight of calling `gr_reply_push_input`.
    pub gr_reply_push_input: u64,

    /// Weight per payload byte by `gr_reply_push_input`.
    pub gr_reply_push_input_per_byte: u64,

    /// Weight of calling `gr_reply_to`.
    pub gr_reply_to: u64,

    /// Weight of calling `gr_signal_from`.
    pub gr_signal_from: u64,

    /// Weight of calling `gr_debug`.
    pub gr_debug: u64,

    /// Weight per payload byte by `gr_debug`.
    pub gr_debug_per_byte: u64,

    /// Weight of calling `gr_reply_code`.
    pub gr_reply_code: u64,

    /// Weight of calling `gr_exit`.
    pub gr_exit: u64,

    /// Weight of calling `gr_leave`.
    pub gr_leave: u64,

    /// Weight of calling `gr_wait`.
    pub gr_wait: u64,

    /// Weight of calling `gr_wait_for`.
    pub gr_wait_for: u64,

    /// Weight of calling `gr_wait_up_to`.
    pub gr_wait_up_to: u64,

    /// Weight of calling `gr_wake`.
    pub gr_wake: u64,

    /// Weight of calling `gr_create_program_wgas`.
    pub gr_create_program: u64,

    /// Weight per payload byte by `gr_create_program_wgas`.
    pub gr_create_program_payload_per_byte: u64,

    /// Weight per salt byte by `gr_create_program_wgas`.
    pub gr_create_program_salt_per_byte: u64,

    /// Weight of calling `gr_create_program_wgas`.
    pub gr_create_program_wgas: u64,

    /// Weight per payload byte by `gr_create_program_wgas`.
    pub gr_create_program_wgas_payload_per_byte: u64,

    /// Weight per salt byte by `gr_create_program_wgas`.
    pub gr_create_program_wgas_salt_per_byte: u64,
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
pub enum RuntimeCosts {
    /// Charge zero gas
    Null,
    /// Weight of calling `alloc` per amount of pages.
    Alloc(u32),
    /// Weight of calling `free`.
    Free,
    /// Weight of calling `gr_reserve_gas`.
    ReserveGas,
    /// Weight of calling `gr_unreserve_gas`.
    UnreserveGas,
    /// Weight of calling `gr_system_reserve_gas`.
    SystemReserveGas,
    /// Weight of calling `gr_gas_available`.
    GasAvailable,
    /// Weight of calling `gr_message_id`.
    MsgId,
    /// Weight of calling `gr_pay_program_rent`.
    PayProgramRent,
    /// Weight of calling `gr_program_id`.
    ProgramId,
    /// Weight of calling `gr_source`.
    Source,
    /// Weight of calling `gr_value`.
    Value,
    /// Weight of calling `gr_value_available`.
    ValueAvailable,
    /// Weight of calling `gr_size`.
    Size,
    /// Weight of calling `gr_read`.
    Read,
    /// Weight of calling `gr_read` per read buffer bytes number.
    ReadPerByte(u32),
    /// Weight of calling `gr_block_height`.
    BlockHeight,
    /// Weight of calling `gr_block_timestamp`.
    BlockTimestamp,
    /// Weight of calling `gr_random`.
    Random,
    /// Weight of calling `gr_reply_deposit`.
    ReplyDeposit,
    /// Weight of calling `gr_send`.
    Send(u32),
    /// Weight of calling `gr_send_wgas`.
    SendWGas(u32),
    /// Weight of calling `gr_send_init`.
    SendInit,
    /// Weight of calling `gr_send_push`.
    SendPush(u32),
    /// Weight of calling `gr_send_commit`.
    SendCommit,
    /// Weight of calling `gr_send_commit_wgas`.
    SendCommitWGas,
    /// Weight of calling `gr_reservation_send`.
    ReservationSend(u32),
    /// Weight of calling `gr_reservation_send_commit`.
    ReservationSendCommit,
    /// Weight of calling `gr_send_input`.
    SendInput,
    /// Weight of calling `gr_send_input_wgas`.
    SendInputWGas,
    /// Weight of calling `gr_send_push_input`.
    SendPushInput,
    /// Weight per buffer bytes number in sent input.
    SendPushInputPerByte(u32),
    /// Weight of calling `gr_reply`.
    Reply(u32),
    /// Weight of calling `gr_reply_wgas`.
    ReplyWGas(u32),
    /// Weight of calling `gr_reply_push`.
    ReplyPush(u32),
    /// Weight of calling `gr_reply_commit`.
    ReplyCommit,
    /// Weight of calling `gr_reply_commit_wgas`.
    ReplyCommitWGas,
    /// Weight of calling `gr_reservation_reply`.
    ReservationReply(u32),
    /// Weight of calling `gr_reservation_reply_commit`.
    ReservationReplyCommit,
    /// Weight of calling `gr_reply_input`.
    ReplyInput,
    /// Weight of calling `gr_reply_input_wgas`.
    ReplyInputWGas,
    /// Weight of calling `gr_reply_push_input`.
    ReplyPushInput,
    /// Weight per buffer bytes number in reply input.
    ReplyPushInputPerByte(u32),
    /// Weight of calling `gr_reply_to`.
    ReplyTo,
    /// Weight of calling `gr_signal_from`.
    SignalFrom,
    /// Weight of calling `gr_debug`.
    Debug(u32),
    /// Weight of calling `gr_reply_code`.
    ReplyCode,
    /// Weight of calling `gr_exit`.
    Exit,
    /// Weight of calling `gr_leave`.
    Leave,
    /// Weight of calling `gr_wait`.
    Wait,
    /// Weight of calling `gr_wait_for`.
    WaitFor,
    /// Weight of calling `gr_wait_up_to`.
    WaitUpTo,
    /// Weight of calling `gr_wake`.
    Wake,
    /// Weight of calling `gr_create_program`.
    CreateProgram(u32, u32),
    /// Weight of calling `gr_create_program_wgas`.
    CreateProgramWGas(u32, u32),
}

impl RuntimeCosts {
    /// Returns a token with a weight given the parameters from `HostFnWeights`.
    pub fn token(&self, s: &HostFnWeights) -> RuntimeToken {
        use self::RuntimeCosts::*;

        let cost_per_byte =
            |weight_per_byte: u64, len: u32| weight_per_byte.saturating_mul(len.into());

        let cost_with_two_weights_per_byte =
            |weight_per_call: u64, weight1_per_byte, weight2_per_byte, len1, len2| {
                weight_per_call
                    .saturating_add(cost_per_byte(weight1_per_byte, len1))
                    .saturating_add(cost_per_byte(weight2_per_byte, len2))
            };

        macro_rules! cost_with_weight_per_byte {
            ($name:ident, $len:expr) => {
                paste! {
                    s.$name.saturating_add(cost_per_byte(s.[< $name _per_byte >], $len))
                }
            };
        }

        let cost_with_weight_per_page = |call_weight: u64, weight_per_page: u64, pages: u32| {
            call_weight.saturating_add(weight_per_page.saturating_mul(pages as u64))
        };

        let weight = match *self {
            Null => 0,
            Alloc(pages) => cost_with_weight_per_page(s.alloc, s.alloc_per_page, pages),
            Free => s.free,
            ReserveGas => s.gr_reserve_gas,
            UnreserveGas => s.gr_unreserve_gas,
            SystemReserveGas => s.gr_system_reserve_gas,
            GasAvailable => s.gr_gas_available,
            MsgId => s.gr_message_id,
            PayProgramRent => s.gr_pay_program_rent,
            ProgramId => s.gr_program_id,
            Source => s.gr_source,
            Value => s.gr_value,
            ValueAvailable => s.gr_value_available,
            Size => s.gr_size,
            Read => s.gr_read,
            ReadPerByte(len) => cost_per_byte(s.gr_read_per_byte, len),
            BlockHeight => s.gr_block_height,
            BlockTimestamp => s.gr_block_timestamp,
            Random => s.gr_random,
            ReplyDeposit => s.gr_reply_deposit,
            Send(len) => cost_with_weight_per_byte!(gr_send, len),
            SendWGas(len) => cost_with_weight_per_byte!(gr_send_wgas, len),
            SendInit => s.gr_send_init,
            SendPush(len) => cost_with_weight_per_byte!(gr_send_push, len),
            SendCommit => s.gr_send_commit,
            SendCommitWGas => s.gr_send_commit_wgas,
            ReservationSend(len) => cost_with_weight_per_byte!(gr_reservation_send, len),
            ReservationSendCommit => s.gr_reservation_send_commit,
            SendInput => s.gr_send_input,
            SendInputWGas => s.gr_send_input_wgas,
            SendPushInput => s.gr_send_push_input,
            SendPushInputPerByte(len) => cost_per_byte(s.gr_send_push_input_per_byte, len),
            Reply(len) => cost_with_weight_per_byte!(gr_reply, len),
            ReplyWGas(len) => cost_with_weight_per_byte!(gr_reply_wgas, len),
            ReplyPush(len) => cost_with_weight_per_byte!(gr_reply_push, len),
            ReplyCommit => s.gr_reply_commit,
            ReplyCommitWGas => s.gr_reply_commit_wgas,
            ReservationReply(len) => cost_with_weight_per_byte!(gr_reservation_reply, len),
            ReservationReplyCommit => s.gr_reservation_reply_commit,
            ReplyInput => s.gr_reply_input,
            ReplyInputWGas => s.gr_reply_input_wgas,
            ReplyPushInput => s.gr_reply_push_input,
            ReplyPushInputPerByte(len) => cost_per_byte(s.gr_reply_push_input_per_byte, len),
            ReplyTo => s.gr_reply_to,
            SignalFrom => s.gr_signal_from,
            Debug(len) => cost_with_weight_per_byte!(gr_debug, len),
            ReplyCode => s.gr_reply_code,
            Exit => s.gr_exit,
            Leave => s.gr_leave,
            Wait => s.gr_wait,
            WaitFor => s.gr_wait_for,
            WaitUpTo => s.gr_wait_up_to,
            Wake => s.gr_wake,
            CreateProgram(payload_len, salt_len) => cost_with_two_weights_per_byte(
                s.gr_create_program,
                s.gr_create_program_payload_per_byte,
                s.gr_create_program_salt_per_byte,
                payload_len,
                salt_len,
            ),
            CreateProgramWGas(payload_len, salt_len) => cost_with_two_weights_per_byte(
                s.gr_create_program_wgas,
                s.gr_create_program_wgas_payload_per_byte,
                s.gr_create_program_wgas_salt_per_byte,
                payload_len,
                salt_len,
            ),
        };
        RuntimeToken { weight }
    }
}
