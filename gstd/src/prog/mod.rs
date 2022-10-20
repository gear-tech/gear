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

//! Program creation module.

mod generator;

pub use generator::ProgramGenerator;

use crate::{
    async_runtime::signals,
    common::errors::Result,
    msg::{CodecCreateProgramFuture, CreateProgramFuture},
    prelude::convert::AsRef,
    ActorId, CodeId, MessageId,
};
use codec::Decode;
use gstd_codegen::wait_create_program_for_reply;

/// Creates new program from already existing on-chain code id,
/// returning initial message id and newly created actor id.
#[wait_create_program_for_reply]
pub fn create_program(
    code_id: CodeId,
    salt: impl AsRef<[u8]>,
    payload: impl AsRef<[u8]>,
    value: u128,
) -> Result<(MessageId, ActorId)> {
    create_program_delayed(code_id, salt, payload, value, 0)
}

/// Same as [`create_program`], but sends delayed.
pub fn create_program_delayed(
    code_id: CodeId,
    salt: impl AsRef<[u8]>,
    payload: impl AsRef<[u8]>,
    value: u128,
    delay: u32,
) -> Result<(MessageId, ActorId)> {
    let (message_id, program_id) = gcore::prog::create_program_delayed(
        code_id.into(),
        salt.as_ref(),
        payload.as_ref(),
        value,
        delay,
    )?;

    Ok((message_id.into(), program_id.into()))
}

/// Same as [`create_program`], but with explicit gas limit.
#[wait_create_program_for_reply]
pub fn create_program_with_gas(
    code_id: CodeId,
    salt: impl AsRef<[u8]>,
    payload: impl AsRef<[u8]>,
    gas_limit: u64,
    value: u128,
) -> Result<(MessageId, ActorId)> {
    create_program_with_gas_delayed(code_id, salt, payload, gas_limit, value, 0)
}

/// Same as [`create_program_with_gas`], but sends delayed.
pub fn create_program_with_gas_delayed(
    code_id: CodeId,
    salt: impl AsRef<[u8]>,
    payload: impl AsRef<[u8]>,
    gas_limit: u64,
    value: u128,
    delay: u32,
) -> Result<(MessageId, ActorId)> {
    let (message_id, program_id) = gcore::prog::create_program_with_gas_delayed(
        code_id.into(),
        salt.as_ref(),
        payload.as_ref(),
        gas_limit,
        value,
        delay,
    )?;

    Ok((message_id.into(), program_id.into()))
}
