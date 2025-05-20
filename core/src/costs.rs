// This file is part of Gear.

// Copyright (C) 2022-2025 Gear Technologies Inc.
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

use crate::pages::{GearPagesAmount, WasmPagesAmount};
use core::{fmt::Debug, marker::PhantomData};
use paste::paste;

/// Gas cost per some type of action or data size.
#[derive(Clone, Copy, PartialEq, Eq)]
pub struct CostOf<T> {
    cost: u64,
    _phantom: PhantomData<T>,
}

impl<T> CostOf<T> {
    /// Const constructor
    pub const fn new(cost: u64) -> Self {
        Self {
            cost,
            _phantom: PhantomData,
        }
    }

    /// Cost for one.
    pub const fn cost_for_one(&self) -> u64 {
        self.cost
    }
}

impl<T: Into<u32>> CostOf<T> {
    /// Calculate (saturating mult) cost for `num` amount of `T`.
    pub fn cost_for(&self, num: T) -> u64 {
        self.cost.saturating_mul(Into::<u32>::into(num).into())
    }
}

impl<T> Debug for CostOf<T> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_fmt(format_args!("{}", &self.cost))
    }
}

impl<T> From<u64> for CostOf<T> {
    fn from(cost: u64) -> Self {
        CostOf::new(cost)
    }
}

impl<T> From<CostOf<T>> for u64 {
    fn from(value: CostOf<T>) -> Self {
        value.cost
    }
}

impl<T> Default for CostOf<T> {
    fn default() -> Self {
        CostOf::new(0)
    }
}

/// Some actions or calls amount.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, derive_more::From, derive_more::Into)]
pub struct CallsAmount(u32);

impl CostOf<CallsAmount> {
    /// Calculate (saturating add) cost for `per_byte` amount of `BytesAmount` (saturating mul).
    pub fn cost_for_with_bytes(&self, per_byte: CostOf<BytesAmount>, amount: BytesAmount) -> u64 {
        self.cost_for_one()
            .saturating_add(per_byte.cost_for(amount))
    }
}

/// Bytes amount.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, derive_more::From, derive_more::Into)]
pub struct BytesAmount(u32);

/// Chain blocks amount.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, derive_more::From, derive_more::Into)]
pub struct BlocksAmount(u32);

/// Program imported function call (syscall) costs.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SyscallCosts {
    /// Cost of calling `alloc`.
    pub alloc: CostOf<CallsAmount>,

    /// Cost of calling `free`.
    pub free: CostOf<CallsAmount>,

    /// Cost of calling `free_range`
    pub free_range: CostOf<CallsAmount>,

    /// Cost of calling `free_range` per page
    pub free_range_per_page: CostOf<WasmPagesAmount>,

    /// Cost of calling `gr_reserve_gas`.
    pub gr_reserve_gas: CostOf<CallsAmount>,

    /// Cost of calling `gr_unreserve_gas`
    pub gr_unreserve_gas: CostOf<CallsAmount>,

    /// Cost of calling `gr_system_reserve_gas`
    pub gr_system_reserve_gas: CostOf<CallsAmount>,

    /// Cost of calling `gr_gas_available`.
    pub gr_gas_available: CostOf<CallsAmount>,

    /// Cost of calling `gr_message_id`.
    pub gr_message_id: CostOf<CallsAmount>,

    /// Cost of calling `gr_program_id`.
    pub gr_program_id: CostOf<CallsAmount>,

    /// Cost of calling `gr_source`.
    pub gr_source: CostOf<CallsAmount>,

    /// Cost of calling `gr_value`.
    pub gr_value: CostOf<CallsAmount>,

    /// Cost of calling `gr_value_available`.
    pub gr_value_available: CostOf<CallsAmount>,

    /// Cost of calling `gr_size`.
    pub gr_size: CostOf<CallsAmount>,

    /// Cost of calling `gr_read`.
    pub gr_read: CostOf<CallsAmount>,

    /// Cost per payload byte for `gr_read`.
    pub gr_read_per_byte: CostOf<BytesAmount>,

    /// Cost of calling `gr_env_vars`.
    pub gr_env_vars: CostOf<CallsAmount>,

    /// Cost of calling `gr_block_height`.
    pub gr_block_height: CostOf<CallsAmount>,

    /// Cost of calling `gr_block_timestamp`.
    pub gr_block_timestamp: CostOf<CallsAmount>,

    /// Cost of calling `gr_random`.
    pub gr_random: CostOf<CallsAmount>,

    /// Cost of calling `gr_reply_deposit`.
    pub gr_reply_deposit: CostOf<CallsAmount>,

    /// Cost of calling `gr_send`
    pub gr_send: CostOf<CallsAmount>,

    /// Cost per bytes for `gr_send`.
    pub gr_send_per_byte: CostOf<BytesAmount>,

    /// Cost of calling `gr_send_wgas`.
    pub gr_send_wgas: CostOf<CallsAmount>,

    /// Cost of calling `gr_send_wgas` per one payload byte.
    pub gr_send_wgas_per_byte: CostOf<BytesAmount>,

    /// Cost of calling `gr_send_init`.
    pub gr_send_init: CostOf<CallsAmount>,

    /// Cost of calling `gr_send_push`.
    pub gr_send_push: CostOf<CallsAmount>,

    /// Cost per payload byte by `gr_send_push`.
    pub gr_send_push_per_byte: CostOf<BytesAmount>,

    /// Cost of calling `gr_send_commit`.
    pub gr_send_commit: CostOf<CallsAmount>,

    /// Cost of calling `gr_send_commit_wgas`.
    pub gr_send_commit_wgas: CostOf<CallsAmount>,

    /// Cost of calling `gr_reservation_send`.
    pub gr_reservation_send: CostOf<CallsAmount>,

    /// Cost of calling `gr_reservation_send` per one payload byte.
    pub gr_reservation_send_per_byte: CostOf<BytesAmount>,

    /// Cost of calling `gr_reservation_send_commit`.
    pub gr_reservation_send_commit: CostOf<CallsAmount>,

    /// Cost of calling `gr_send_init`.
    pub gr_send_input: CostOf<CallsAmount>,

    /// Cost of calling `gr_send_init_wgas`.
    pub gr_send_input_wgas: CostOf<CallsAmount>,

    /// Cost of calling `gr_send_push_input`.
    pub gr_send_push_input: CostOf<CallsAmount>,

    /// Cost per payload byte by `gr_send_push_input`.
    pub gr_send_push_input_per_byte: CostOf<BytesAmount>,

    /// Cost of calling `gr_reply`.
    pub gr_reply: CostOf<CallsAmount>,

    /// Cost of calling `gr_reply` per one payload byte.
    pub gr_reply_per_byte: CostOf<BytesAmount>,

    /// Cost of calling `gr_reply_wgas`.
    pub gr_reply_wgas: CostOf<CallsAmount>,

    /// Cost of calling `gr_reply_wgas` per one payload byte.
    pub gr_reply_wgas_per_byte: CostOf<BytesAmount>,

    /// Cost of calling `gr_reply_commit`.
    pub gr_reply_commit: CostOf<CallsAmount>,

    /// Cost of calling `gr_reply_commit_wgas`.
    pub gr_reply_commit_wgas: CostOf<CallsAmount>,

    /// Cost of calling `gr_reservation_reply`.
    pub gr_reservation_reply: CostOf<CallsAmount>,

    /// Cost of calling `gr_reservation_reply` per one payload byte.
    pub gr_reservation_reply_per_byte: CostOf<BytesAmount>,

    /// Cost of calling `gr_reservation_reply_commit`.
    pub gr_reservation_reply_commit: CostOf<CallsAmount>,

    /// Cost of calling `gr_reply_push`.
    pub gr_reply_push: CostOf<CallsAmount>,

    /// Cost per payload byte by `gr_reply_push`.
    pub gr_reply_push_per_byte: CostOf<BytesAmount>,

    /// Cost of calling `gr_reply_input`.
    pub gr_reply_input: CostOf<CallsAmount>,

    /// Cost of calling `gr_reply_input_wgas`.
    pub gr_reply_input_wgas: CostOf<CallsAmount>,

    /// Cost of calling `gr_reply_push_input`.
    pub gr_reply_push_input: CostOf<CallsAmount>,

    /// Cost per payload byte by `gr_reply_push_input`.
    pub gr_reply_push_input_per_byte: CostOf<BytesAmount>,

    /// Cost of calling `gr_reply_to`.
    pub gr_reply_to: CostOf<CallsAmount>,

    /// Cost of calling `gr_signal_code`.
    pub gr_signal_code: CostOf<CallsAmount>,

    /// Cost of calling `gr_signal_from`.
    pub gr_signal_from: CostOf<CallsAmount>,

    /// Cost of calling `gr_debug`.
    pub gr_debug: CostOf<CallsAmount>,

    /// Cost per payload byte by `gr_debug`.
    pub gr_debug_per_byte: CostOf<BytesAmount>,

    /// Cost of calling `gr_reply_code`.
    pub gr_reply_code: CostOf<CallsAmount>,

    /// Cost of calling `gr_exit`.
    pub gr_exit: CostOf<CallsAmount>,

    /// Cost of calling `gr_leave`.
    pub gr_leave: CostOf<CallsAmount>,

    /// Cost of calling `gr_wait`.
    pub gr_wait: CostOf<CallsAmount>,

    /// Cost of calling `gr_wait_for`.
    pub gr_wait_for: CostOf<CallsAmount>,

    /// Cost of calling `gr_wait_up_to`.
    pub gr_wait_up_to: CostOf<CallsAmount>,

    /// Cost of calling `gr_wake`.
    pub gr_wake: CostOf<CallsAmount>,

    /// Cost of calling `gr_create_program_wgas`.
    pub gr_create_program: CostOf<CallsAmount>,

    /// Cost per payload byte by `gr_create_program_wgas`.
    pub gr_create_program_payload_per_byte: CostOf<BytesAmount>,

    /// Cost per salt byte by `gr_create_program_wgas`.
    pub gr_create_program_salt_per_byte: CostOf<BytesAmount>,

    /// Cost of calling `gr_create_program_wgas`.
    pub gr_create_program_wgas: CostOf<CallsAmount>,

    /// Cost per payload byte by `gr_create_program_wgas`.
    pub gr_create_program_wgas_payload_per_byte: CostOf<BytesAmount>,

    /// Cost per salt byte by `gr_create_program_wgas`.
    pub gr_create_program_wgas_salt_per_byte: CostOf<BytesAmount>,
}

/// Enumerates syscalls that can be charged by gas meter.
#[derive(Debug, Copy, Clone)]
pub enum CostToken {
    /// Zero cost.
    Null,
    /// Cost of calling `alloc`.
    Alloc,
    /// Cost of calling `free`.
    Free,
    /// Cost of calling `free_range`
    FreeRange,
    /// Cost of calling `gr_reserve_gas`.
    ReserveGas,
    /// Cost of calling `gr_unreserve_gas`.
    UnreserveGas,
    /// Cost of calling `gr_system_reserve_gas`.
    SystemReserveGas,
    /// Cost of calling `gr_gas_available`.
    GasAvailable,
    /// Cost of calling `gr_message_id`.
    MsgId,
    /// Cost of calling `gr_program_id`.
    ActorId,
    /// Cost of calling `gr_source`.
    Source,
    /// Cost of calling `gr_value`.
    Value,
    /// Cost of calling `gr_value_available`.
    ValueAvailable,
    /// Cost of calling `gr_size`.
    Size,
    /// Cost of calling `gr_read`.
    Read,
    /// Cost of calling `gr_env_vars`.
    EnvVars,
    /// Cost of calling `gr_block_height`.
    BlockHeight,
    /// Cost of calling `gr_block_timestamp`.
    BlockTimestamp,
    /// Cost of calling `gr_random`.
    Random,
    /// Cost of calling `gr_reply_deposit`.
    ReplyDeposit,
    /// Cost of calling `gr_send`, taking in account payload size.
    Send(BytesAmount),
    /// Cost of calling `gr_send_wgas`, taking in account payload size.
    SendWGas(BytesAmount),
    /// Cost of calling `gr_send_init`.
    SendInit,
    /// Cost of calling `gr_send_push`, taking in account payload size.
    SendPush(BytesAmount),
    /// Cost of calling `gr_send_commit`.
    SendCommit,
    /// Cost of calling `gr_send_commit_wgas`.
    SendCommitWGas,
    /// Cost of calling `gr_reservation_send`, taking in account payload size.
    ReservationSend(BytesAmount),
    /// Cost of calling `gr_reservation_send_commit`.
    ReservationSendCommit,
    /// Cost of calling `gr_send_input`.
    SendInput,
    /// Cost of calling `gr_send_input_wgas`.
    SendInputWGas,
    /// Cost of calling `gr_send_push_input`.
    SendPushInput,
    /// Cost of calling `gr_reply`, taking in account payload size.
    Reply(BytesAmount),
    /// Cost of calling `gr_reply_wgas`, taking in account payload size.
    ReplyWGas(BytesAmount),
    /// Cost of calling `gr_reply_push`, taking in account payload size.
    ReplyPush(BytesAmount),
    /// Cost of calling `gr_reply_commit`.
    ReplyCommit,
    /// Cost of calling `gr_reply_commit_wgas`.
    ReplyCommitWGas,
    /// Cost of calling `gr_reservation_reply`, taking in account payload size.
    ReservationReply(BytesAmount),
    /// Cost of calling `gr_reservation_reply_commit`.
    ReservationReplyCommit,
    /// Cost of calling `gr_reply_input`.
    ReplyInput,
    /// Cost of calling `gr_reply_input_wgas`.
    ReplyInputWGas,
    /// Cost of calling `gr_reply_push_input`.
    ReplyPushInput,
    /// Cost of calling `gr_reply_to`.
    ReplyTo,
    /// Cost of calling `gr_signal_code`.
    SignalCode,
    /// Cost of calling `gr_signal_from`.
    SignalFrom,
    /// Cost of calling `gr_debug`, taking in account payload size.
    Debug(BytesAmount),
    /// Cost of calling `gr_reply_code`.
    ReplyCode,
    /// Cost of calling `gr_exit`.
    Exit,
    /// Cost of calling `gr_leave`.
    Leave,
    /// Cost of calling `gr_wait`.
    Wait,
    /// Cost of calling `gr_wait_for`.
    WaitFor,
    /// Cost of calling `gr_wait_up_to`.
    WaitUpTo,
    /// Cost of calling `gr_wake`.
    Wake,
    /// Cost of calling `gr_create_program`, taking in account payload and salt size.
    CreateProgram(BytesAmount, BytesAmount),
    /// Cost of calling `gr_create_program_wgas`, taking in account payload and salt size.
    CreateProgramWGas(BytesAmount, BytesAmount),
}

impl SyscallCosts {
    /// Get cost for a token.
    pub fn cost_for_token(&self, token: CostToken) -> u64 {
        use CostToken::*;

        macro_rules! cost_with_per_byte {
            ($name:ident, $len:expr) => {
                paste! {
                    self.$name.cost_for_with_bytes(self.[< $name _per_byte >], $len)
                }
            };
        }

        match token {
            Null => 0,
            Alloc => self.alloc.cost_for_one(),
            Free => self.free.cost_for_one(),
            FreeRange => self.free_range.cost_for_one(),
            ReserveGas => self.gr_reserve_gas.cost_for_one(),
            UnreserveGas => self.gr_unreserve_gas.cost_for_one(),
            SystemReserveGas => self.gr_system_reserve_gas.cost_for_one(),
            GasAvailable => self.gr_gas_available.cost_for_one(),
            MsgId => self.gr_message_id.cost_for_one(),
            ActorId => self.gr_program_id.cost_for_one(),
            Source => self.gr_source.cost_for_one(),
            Value => self.gr_value.cost_for_one(),
            ValueAvailable => self.gr_value_available.cost_for_one(),
            Size => self.gr_size.cost_for_one(),
            Read => self.gr_read.cost_for_one(),
            EnvVars => self.gr_env_vars.cost_for_one(),
            BlockHeight => self.gr_block_height.cost_for_one(),
            BlockTimestamp => self.gr_block_timestamp.cost_for_one(),
            Random => self.gr_random.cost_for_one(),
            ReplyDeposit => self.gr_reply_deposit.cost_for_one(),
            Send(len) => cost_with_per_byte!(gr_send, len),
            SendWGas(len) => cost_with_per_byte!(gr_send_wgas, len),
            SendInit => self.gr_send_init.cost_for_one(),
            SendPush(len) => cost_with_per_byte!(gr_send_push, len),
            SendCommit => self.gr_send_commit.cost_for_one(),
            SendCommitWGas => self.gr_send_commit_wgas.cost_for_one(),
            ReservationSend(len) => cost_with_per_byte!(gr_reservation_send, len),
            ReservationSendCommit => self.gr_reservation_send_commit.cost_for_one(),
            SendInput => self.gr_send_input.cost_for_one(),
            SendInputWGas => self.gr_send_input_wgas.cost_for_one(),
            SendPushInput => self.gr_send_push_input.cost_for_one(),
            Reply(len) => cost_with_per_byte!(gr_reply, len),
            ReplyWGas(len) => cost_with_per_byte!(gr_reply_wgas, len),
            ReplyPush(len) => cost_with_per_byte!(gr_reply_push, len),
            ReplyCommit => self.gr_reply_commit.cost_for_one(),
            ReplyCommitWGas => self.gr_reply_commit_wgas.cost_for_one(),
            ReservationReply(len) => cost_with_per_byte!(gr_reservation_reply, len),
            ReservationReplyCommit => self.gr_reservation_reply_commit.cost_for_one(),
            ReplyInput => self.gr_reply_input.cost_for_one(),
            ReplyInputWGas => self.gr_reply_input_wgas.cost_for_one(),
            ReplyPushInput => self.gr_reply_push_input.cost_for_one(),
            ReplyTo => self.gr_reply_to.cost_for_one(),
            SignalCode => self.gr_signal_code.cost_for_one(),
            SignalFrom => self.gr_signal_from.cost_for_one(),
            Debug(len) => cost_with_per_byte!(gr_debug, len),
            ReplyCode => self.gr_reply_code.cost_for_one(),
            Exit => self.gr_exit.cost_for_one(),
            Leave => self.gr_leave.cost_for_one(),
            Wait => self.gr_wait.cost_for_one(),
            WaitFor => self.gr_wait_for.cost_for_one(),
            WaitUpTo => self.gr_wait_up_to.cost_for_one(),
            Wake => self.gr_wake.cost_for_one(),
            CreateProgram(payload, salt) => CostOf::from(
                self.gr_create_program
                    .cost_for_with_bytes(self.gr_create_program_payload_per_byte, payload),
            )
            .cost_for_with_bytes(self.gr_create_program_salt_per_byte, salt),
            CreateProgramWGas(payload, salt) => CostOf::from(
                self.gr_create_program_wgas
                    .cost_for_with_bytes(self.gr_create_program_wgas_payload_per_byte, payload),
            )
            .cost_for_with_bytes(self.gr_create_program_wgas_salt_per_byte, salt),
        }
    }
}

/// Memory pages costs.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct PagesCosts {
    /// Loading from storage and moving it in program memory cost.
    pub load_page_data: CostOf<GearPagesAmount>,
    /// Uploading page data to storage cost.
    pub upload_page_data: CostOf<GearPagesAmount>,
    /// Memory grow cost.
    pub mem_grow: CostOf<GearPagesAmount>,
    /// Memory grow per page cost.
    pub mem_grow_per_page: CostOf<GearPagesAmount>,
    /// Parachain read heuristic cost.
    pub parachain_read_heuristic: CostOf<GearPagesAmount>,
}

/// Memory pages lazy access costs.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct LazyPagesCosts {
    /// First read page access cost.
    pub signal_read: CostOf<GearPagesAmount>,
    /// First write page access cost.
    pub signal_write: CostOf<GearPagesAmount>,
    /// First write access cost for page, which has been already read accessed.
    pub signal_write_after_read: CostOf<GearPagesAmount>,
    /// First read page access cost from host function call.
    pub host_func_read: CostOf<GearPagesAmount>,
    /// First write page access cost from host function call.
    pub host_func_write: CostOf<GearPagesAmount>,
    /// First write page access cost from host function call.
    pub host_func_write_after_read: CostOf<GearPagesAmount>,
    /// Loading page data from storage cost.
    pub load_page_storage_data: CostOf<GearPagesAmount>,
}

/// IO costs.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct IoCosts {
    /// Consts for common pages.
    pub common: PagesCosts,
    /// Consts for lazy pages.
    pub lazy_pages: LazyPagesCosts,
}

/// Holding in storages rent costs.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct RentCosts {
    /// Holding message in waitlist cost per block.
    pub waitlist: CostOf<BlocksAmount>,
    /// Holding message in dispatch stash cost per block.
    pub dispatch_stash: CostOf<BlocksAmount>,
    /// Holding reservation cost per block.
    pub reservation: CostOf<BlocksAmount>,
}

/// Execution externalities costs.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct ExtCosts {
    /// Syscalls costs.
    pub syscalls: SyscallCosts,
    /// Rent costs.
    pub rent: RentCosts,
    /// Memory grow cost.
    pub mem_grow: CostOf<CallsAmount>,
    /// Memory grow per page cost.
    pub mem_grow_per_page: CostOf<WasmPagesAmount>,
}

/// Module instantiation costs.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct InstantiationCosts {
    /// WASM module code section instantiation per byte cost.
    pub code_section_per_byte: CostOf<BytesAmount>,
    /// WASM module data section instantiation per byte cost.
    pub data_section_per_byte: CostOf<BytesAmount>,
    /// WASM module global section instantiation per byte cost.
    pub global_section_per_byte: CostOf<BytesAmount>,
    /// WASM module table section instantiation per byte cost.
    pub table_section_per_byte: CostOf<BytesAmount>,
    /// WASM module element section instantiation per byte cost.
    pub element_section_per_byte: CostOf<BytesAmount>,
    /// WASM module type section instantiation per byte cost.
    pub type_section_per_byte: CostOf<BytesAmount>,
}

/// Costs for message processing
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ProcessCosts {
    /// Execution externalities costs.
    pub ext: ExtCosts,
    /// Lazy pages costs.
    pub lazy_pages: LazyPagesCosts,
    /// Storage read cost.
    pub read: CostOf<CallsAmount>,
    /// Storage read per byte cost.
    pub read_per_byte: CostOf<BytesAmount>,
    /// Storage write cost.
    pub write: CostOf<CallsAmount>,
    /// Code instrumentation cost.
    pub instrumentation: CostOf<CallsAmount>,
    /// Code instrumentation per byte cost.
    pub instrumentation_per_byte: CostOf<BytesAmount>,
    /// Module instantiation costs.
    pub instantiation_costs: InstantiationCosts,
    /// Load program allocations cost per interval.
    pub load_allocations_per_interval: CostOf<u32>,
}
