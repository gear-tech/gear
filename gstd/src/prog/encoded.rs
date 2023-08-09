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

use crate::{
    async_runtime::signals,
    common::errors::Result,
    msg::{CodecCreateProgramFuture, CreateProgramFuture},
    prelude::convert::AsRef,
    ActorId, CodeId, MessageId,
};
use gstd_codegen::wait_create_program_for_reply;
use scale_info::scale::{Decode, Encode};

/// TODO.
#[wait_create_program_for_reply]
pub fn create_program<E: Encode>(
    code_id: CodeId,
    salt: impl AsRef<[u8]>,
    payload: E,
    value: u128,
) -> Result<(MessageId, ActorId)> {
    super::create_program_bytes(code_id, salt, payload.encode(), value)
}

/// TODO.
pub fn create_program_delayed<E: Encode>(
    code_id: CodeId,
    salt: impl AsRef<[u8]>,
    payload: E,
    value: u128,
    delay: u32,
) -> Result<(MessageId, ActorId)> {
    super::create_program_bytes_delayed(code_id, salt, payload.encode(), value, delay)
}

/// TODO.
#[wait_create_program_for_reply]
pub fn create_program_with_gas<E: Encode>(
    code_id: CodeId,
    salt: impl AsRef<[u8]>,
    payload: E,
    gas_limit: u64,
    value: u128,
) -> Result<(MessageId, ActorId)> {
    super::create_program_bytes_with_gas(code_id, salt, payload.encode(), gas_limit, value)
}

/// TODO.
pub fn create_program_with_gas_delayed<E: Encode>(
    code_id: CodeId,
    salt: impl AsRef<[u8]>,
    payload: E,
    gas_limit: u64,
    value: u128,
    delay: u32,
) -> Result<(MessageId, ActorId)> {
    super::create_program_bytes_with_gas_delayed(
        code_id,
        salt,
        payload.encode(),
        gas_limit,
        value,
        delay,
    )
}
