// This file is part of Gear.

// Copyright (C) 2021-2025 Gear Technologies Inc.
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

use crate::{ActorId, CodeId, MessageId};
use gcore::errors::Result;
use gstd_codegen::wait_create_program_for_reply;

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
/// use gstd::{CodeId, msg, prog};
///
/// #[unsafe(no_mangle)]
/// extern "C" fn handle() {
///     let code_id: CodeId = msg::load().expect("Unable to load");
///     let (init_message_id, new_program_id) =
///         prog::create_program_bytes(code_id, "salt", b"INIT", 0)
///             .expect("Unable to create a program");
///     msg::send_bytes(new_program_id, b"PING", 0).expect("Unable to send");
/// }
/// ```
#[wait_create_program_for_reply]
pub fn create_program_bytes(
    code_id: CodeId,
    salt: impl AsRef<[u8]>,
    payload: impl AsRef<[u8]>,
    value: u128,
) -> Result<(MessageId, ActorId)> {
    create_program_bytes_delayed(code_id, salt, payload, value, 0)
}

/// Same as [`create_program_bytes`], but creates a new program after the
/// `delay` expressed in block count.
///
/// # Examples
///
/// Create a new program from the provided code identifier after 100 blocks and
/// send a message to it after 200 blocks:
///
/// ```
/// use gstd::{CodeId, msg, prog};
///
/// #[unsafe(no_mangle)]
/// extern "C" fn handle() {
///     let code_id: CodeId = msg::load().expect("Unable to load");
///     let (init_message_id, new_program_id) =
///         prog::create_program_bytes_delayed(code_id, "salt", b"INIT", 0, 100)
///             .expect("Unable to create a program");
///     msg::send_bytes_delayed(new_program_id, b"PING", 0, 200).expect("Unable to send");
/// }
/// ```
pub fn create_program_bytes_delayed(
    code_id: CodeId,
    salt: impl AsRef<[u8]>,
    payload: impl AsRef<[u8]>,
    value: u128,
    delay: u32,
) -> Result<(MessageId, ActorId)> {
    gcore::prog::create_program_delayed(code_id, salt.as_ref(), payload.as_ref(), value, delay)
}

/// Same as [`create_program_bytes`], but with an explicit gas limit.
#[cfg(not(feature = "gearexe"))]
#[wait_create_program_for_reply]
pub fn create_program_bytes_with_gas(
    code_id: CodeId,
    salt: impl AsRef<[u8]>,
    payload: impl AsRef<[u8]>,
    gas_limit: u64,
    value: u128,
) -> Result<(MessageId, ActorId)> {
    create_program_bytes_with_gas_delayed(code_id, salt, payload, gas_limit, value, 0)
}

/// Same as [`create_program_bytes_with_gas`], but creates a new program after
/// the `delay` expressed in block count.
#[cfg(not(feature = "gearexe"))]
pub fn create_program_bytes_with_gas_delayed(
    code_id: CodeId,
    salt: impl AsRef<[u8]>,
    payload: impl AsRef<[u8]>,
    gas_limit: u64,
    value: u128,
    delay: u32,
) -> Result<(MessageId, ActorId)> {
    gcore::prog::create_program_with_gas_delayed(
        code_id,
        salt.as_ref(),
        payload.as_ref(),
        gas_limit,
        value,
        delay,
    )
}
