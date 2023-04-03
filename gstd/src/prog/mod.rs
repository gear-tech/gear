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

//! Functions and helpers for creating programs from programs.
//!
//! Any program being an actor, can not only process incoming messages and send
//! outcoming messages to other actors but also create new actors. This feature
//! can be useful when implementing the factory pattern, as a single
//! actor can produce multiple derived actors with different input data.
//!
//! Firstly you need to upload a Wasm code of the future program(s) by calling
//! `gear.uploadCode` extrinsic to obtain the corresponding [`CodeId`].
//!
//! You must also provide a unique byte sequence to create multiple program
//! instances from the same code. This sequence is often referenced as _salt_.
//! [`ProgramGenerator`] allows generating of salt automatically.
//!
//! The newly created program should be initialized using a corresponding
//! payload; therefore, you must provide it when calling any `create_program_*`
//! function.

mod generator;

pub use generator::ProgramGenerator;

use crate::{
    async_runtime::signals,
    common::errors::Result,
    msg::{CodecCreateProgramFuture, CreateProgramFuture},
    prelude::convert::AsRef,
    ActorId, CodeId, MessageId,
};
use gstd_codegen::wait_create_program_for_reply;
use scale_info::scale::Decode;

/// Create a new program from the already existing on-chain code identified by
/// [`CodeId`].
///
/// The function returns an initial message identifier and a newly created
/// program identifier.
///
/// The first argument is the code identifier (see [`CodeId`] for details). The
/// second argument is an arbitrary byte sequence (also known as `salt`) that
/// allows the creation of multiple programs from the same code. The third and
/// last arguments are the initialization message's payload and value to be
/// transferred to the newly created program.
///
/// # Examples
///
/// Create a new program from the provided code identifier:
///
/// ```
/// use gstd::{msg, prog, CodeId};
///
/// #[no_mangle]
/// extern "C" fn handle() {
///     let code_id: CodeId = msg::load().expect("Unable to load");
///     let (init_message_id, new_program_id) =
///         prog::create_program(code_id, "salt", b"INIT", 0).expect("Unable to create a program");
///     msg::send_bytes(new_program_id, b"PING", 0).expect("Unable to send");
/// }
/// ```
#[wait_create_program_for_reply]
pub fn create_program(
    code_id: CodeId,
    salt: impl AsRef<[u8]>,
    payload: impl AsRef<[u8]>,
    value: u128,
) -> Result<(MessageId, ActorId)> {
    create_program_delayed(code_id, salt, payload, value, 0)
}

/// Same as [`create_program`], but creates a new program after the `delay`
/// expressed in block count.
///
/// # Examples
///
/// Create a new program from the provided code identifier after 100 blocks and
/// send a message to it after 200 blocks:
///
/// ```
/// use gstd::{msg, prog, CodeId};
///
/// #[no_mangle]
/// extern "C" fn handle() {
///     let code_id: CodeId = msg::load().expect("Unable to load");
///     let (init_message_id, new_program_id) =
///         prog::create_program_delayed(code_id, "salt", b"INIT", 0, 100)
///             .expect("Unable to create a program");
///     msg::send_bytes_delayed(new_program_id, b"PING", 0, 200).expect("Unable to send");
/// }
/// ```
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

/// Same as [`create_program`], but with an explicit gas limit.
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

/// Same as [`create_program_with_gas`], but creates a new program after the
/// `delay` expressed in block count.
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
