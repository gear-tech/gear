// This file is part of Gear.

// Copyright (C) 2022 Gear Technologies Inc.
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
use alloc::{vec, vec::Vec};

/// Syscall param type.
#[derive(Debug, Clone, Copy)]
pub enum ParamType {
    Size,            // i32 buffers size in memory
    Ptr,             // i32 pointer
    Gas,             // i64 gas amount
    MessagePosition, // i32 message position
    Duration,        // i32 duration in blocks
    Delay,           // i32 delay in blocks
    Handler,         // i32 handler number
    Alloc,           // i32 alloc pages
    Free,            // i32 free page
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
#[derive(Debug, Clone)]
pub struct SysCallSignature {
    pub params: Vec<ParamType>,
    pub results: Vec<ValueType>,
}

impl SysCallSignature {
    fn new<const N: usize, const M: usize>(
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

/// Returns list of all syscall names (actually supported by this module syscalls).
pub fn syscalls_name_list() -> Vec<&'static str> {
    vec![
        "alloc",
        "free",
        "gr_debug",
        "gr_error",
        "gr_block_height",
        "gr_block_timestamp",
        "gr_exit",
        "gr_gas_available",
        "gr_program_id",
        "gr_origin",
        "gr_leave",
        "gr_value_available",
        "gr_wait",
        "gr_wait_up_to",
        "gr_wait_for",
        "gr_wake",
        "gr_exit_code",
        "gr_message_id",
        "gr_read",
        "gr_reply",
        "gr_reply_wgas",
        "gr_reply_commit",
        "gr_reply_commit_wgas",
        "gr_reservation_reply",
        "gr_reservation_reply_commit",
        "gr_reply_push",
        "gr_reply_to",
        "gr_send",
        "gr_send_wgas",
        "gr_send_commit",
        "gr_send_commit_wgas",
        "gr_send_init",
        "gr_send_push",
        "gr_reservation_send",
        "gr_reservation_send_commit",
        "gr_size",
        "gr_source",
        "gr_value",
        "gr_create_program",
        "gr_create_program_wgas",
        "gr_reserve_gas",
        "gr_unreserve_gas",
        "gr_random",
    ]
}

/// Returns signature for syscall by name.
pub fn syscall_signature(name: &str) -> SysCallSignature {
    use ParamType::*;
    use ValueType::{I32, I64};
    match name {
        "alloc" => SysCallSignature::new([Alloc], [I32]),
        "free" => SysCallSignature::new([Free], []),
        "gr_debug" => SysCallSignature::new([Ptr, Size], []),
        "gr_error" => SysCallSignature::new([Ptr], [I32]),
        "gr_block_height" => SysCallSignature::new([], [I32]),
        "gr_block_timestamp" => SysCallSignature::new([], [I64]),
        "gr_exit" => SysCallSignature::new([Ptr], []),
        "gr_gas_available" => SysCallSignature::new([], [I64]),
        "gr_program_id" => SysCallSignature::new([Ptr], []),
        "gr_origin" => SysCallSignature::new([Ptr], []),
        "gr_leave" => SysCallSignature::new([], []),
        "gr_value_available" => SysCallSignature::new([Ptr], []),
        "gr_wait" => SysCallSignature::new([], []),
        "gr_wait_up_to" => SysCallSignature::new([Duration], []),
        "gr_wait_for" => SysCallSignature::new([Duration], []),
        "gr_wake" => SysCallSignature::new([Ptr, Delay], [I32]),
        "gr_exit_code" => SysCallSignature::new([Ptr], [I32]),
        "gr_message_id" => SysCallSignature::new([Ptr], []),
        "gr_read" => SysCallSignature::new([MessagePosition, Size, Ptr], [I32]),
        "gr_reply" => SysCallSignature::new([Ptr, Size, Ptr, Ptr, Delay], [I32]),
        "gr_reply_wgas" => SysCallSignature::new([Ptr, Size, Gas, Ptr, Delay, Ptr], [I32]),
        "gr_reply_commit" => SysCallSignature::new([Ptr, Delay, Ptr], [I32]),
        "gr_reply_commit_wgas" => SysCallSignature::new([Gas, Ptr, Delay, Ptr], [I32]),
        "gr_reservation_reply" => SysCallSignature::new([Ptr, Ptr, Size, Ptr, Ptr, Delay], [I32]),
        "gr_reservation_reply_commit" => SysCallSignature::new([Ptr, Ptr, Delay, Ptr], [I32]),
        "gr_reply_push" => SysCallSignature::new([Ptr, Size], [I32]),
        "gr_reply_to" => SysCallSignature::new([Ptr], [I32]),
        "gr_rereply_push" => SysCallSignature::new([Size, Size], [I32]),
        "gr_resend_push" => SysCallSignature::new([Handler, Size, Size], [I32]),
        "gr_send" => SysCallSignature::new([Ptr, Ptr, Size, Ptr, Delay, Ptr], [I32]),
        "gr_send_wgas" => SysCallSignature::new([Ptr, Ptr, Size, Gas, Ptr, Delay, Ptr], [I32]),
        "gr_send_commit" => SysCallSignature::new([Handler, Ptr, Ptr, Delay, Ptr], [I32]),
        "gr_send_commit_wgas" => SysCallSignature::new([Handler, Ptr, Gas, Ptr, Delay, Ptr], [I32]),
        "gr_send_init" => SysCallSignature::new([Handler], [I32]),
        "gr_send_push" => SysCallSignature::new([Handler, Ptr, Size], [I32]),
        "gr_reservation_send" => {
            SysCallSignature::new([Ptr, Ptr, Ptr, Size, Ptr, Delay, Ptr], [I32])
        }
        "gr_reservation_send_commit" => {
            SysCallSignature::new([Ptr, Handler, Ptr, Ptr, Delay, Ptr], [I32])
        }
        "gr_size" => SysCallSignature::new([], [I32]),
        "gr_source" => SysCallSignature::new([Ptr], []),
        "gr_value" => SysCallSignature::new([Ptr], []),
        "gr_create_program" => {
            SysCallSignature::new([Ptr, Ptr, Size, Ptr, Size, Ptr, Delay, Ptr, Ptr], [I32])
        }
        "gr_create_program_wgas" => SysCallSignature::new(
            [Ptr, Ptr, Size, Ptr, Size, Gas, Ptr, Delay, Ptr, Ptr],
            [I32],
        ),
        "gr_reserve_gas" => SysCallSignature::new([Gas, Duration, Ptr], [I32]),
        "gr_unreserve_gas" => SysCallSignature::new([Ptr, Ptr], [I32]),
        "gr_random" => SysCallSignature::new([Ptr, Size, Ptr, Ptr], []),
        other => panic!("Unknown syscall name: '{}'", other),
    }
}
