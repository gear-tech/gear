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

//! API for creating programs from Gear programs.
//!
//! Once a Wasm code has been uploaded to the chain, the Gear program can create
//! a new program from the code blob using its identifier.
//!
//! To create a new program, you are to provide the code identifier obtained
//! after running the `upload_code` extrinsic, unique salt (arbitrary data
//! needed to instantiate several programs from one code blob), and the init
//! message that consists at least from a payload and value.

use crate::{
    errors::{Result, SyscallError},
    ActorId, CodeId, MessageId,
};
use gear_core_errors::ExtError;
use gsys::{HashWithValue, LengthWithTwoHashes};

/// Create a new program and returns its address.
///
/// This function creates a program initialization message.
///
/// Parameters:
/// - `code_id` is the code identifier of newly creating program that is
///   represented as blake2b hash;
/// - `salt` is the arbitrary data needed to generate an address for a new
///   program (control of salt uniqueness is entirely on the program developer's
///   side);
/// - `payload` that can be used in the `init` function of the newly deployed
///   "child" program;
/// - `value` sent with the init message.
///
/// # Examples
///
/// Basically we can use "automatic" salt generation ("nonce"):
///
/// ```
/// use gcore::{prog, CodeId};
///
/// static mut NONCE: i32 = 0;
///
/// fn increase() {
///     unsafe { NONCE += 1 };
/// }
///
/// fn get() -> i32 {
///     unsafe { NONCE }
/// }
///
/// #[no_mangle]
/// extern "C" fn handle() {
///     // We assume we already have a code identifier
///     let submitted_code: CodeId =
///         hex_literal::hex!("abf3746e72a6e8740bd9e12b879fbdd59e052cb390f116454e9116c22021ae4a")
///             .into();
///     let (message_id, new_program_id) =
///         prog::create_program(submitted_code, &get().to_le_bytes(), b"", 0)
///             .expect("Unable to create a program");
/// }
/// ```
///
/// Another case for salt is to receive it as input:
///
/// ```
/// use gcore::{msg, prog};
/// # use gcore::CodeId;
///
/// #[no_mangle]
/// extern "C" fn handle() {
///     # let submitted_code: CodeId = hex_literal::hex!("abf3746e72a6e8740bd9e12b879fbdd59e052cb390f116454e9116c22021ae4a").into();
///     // ...
///     let mut salt = vec![0u8; msg::size()];
///     msg::read(&mut salt).expect("Unable to read");
///     let (message_id, new_program_id) = prog::create_program(submitted_code, &salt, b"", 0)
///         .expect("Unable to create a program");
/// }
/// ```
///
/// Moreover, messages can be sent to a new program:
///
/// ```
/// use gcore::{msg, prog};
/// # use gcore::CodeId;
///
/// #[no_mangle]
/// extern "C" fn handle() {
///     # let submitted_code: CodeId = hex_literal::hex!("abf3746e72a6e8740bd9e12b879fbdd59e052cb390f116454e9116c22021ae4a").into();
///     # let mut salt = vec![0u8; msg::size()];
///     # msg::read(&mut salt).unwrap();
///     // ...
///     let (_, new_program_id) = prog::create_program(submitted_code, &salt, b"", 0)
///         .expect("Unable to create a program");
///     msg::send(new_program_id, b"payload for a new program", 0)
///         .expect("Unable to send");
/// }
/// ```
pub fn create_program(
    code_id: CodeId,
    salt: &[u8],
    payload: &[u8],
    value: u128,
) -> Result<(MessageId, ActorId)> {
    create_program_delayed(code_id, salt, payload, value, 0)
}

/// Same as [`create_program`], but with an explicit gas limit.
pub fn create_program_with_gas(
    code_id: CodeId,
    salt: &[u8],
    payload: &[u8],
    gas_limit: u64,
    value: u128,
) -> Result<(MessageId, ActorId)> {
    create_program_with_gas_delayed(code_id, salt, payload, gas_limit, value, 0)
}

/// Same as [`create_program`], but creates a new program after the `delay`
/// expressed in block count.
pub fn create_program_delayed(
    code_id: CodeId,
    salt: &[u8],
    payload: &[u8],
    value: u128,
    delay: u32,
) -> Result<(MessageId, ActorId)> {
    let cid_value = HashWithValue {
        hash: code_id.0,
        value,
    };

    let mut res: LengthWithTwoHashes = Default::default();

    let salt_len = salt.len().try_into().map_err(|_| ExtError::SyscallUsage)?;

    let payload_len = payload
        .len()
        .try_into()
        .map_err(|_| ExtError::SyscallUsage)?;

    unsafe {
        gsys::gr_create_program(
            cid_value.as_ptr(),
            salt.as_ptr(),
            salt_len,
            payload.as_ptr(),
            payload_len,
            delay,
            res.as_mut_ptr(),
        )
    };
    SyscallError(res.length).into_result()?;

    Ok((MessageId(res.hash1), ActorId(res.hash2)))
}

/// Same as [`create_program_with_gas`], but creates a new program after the
/// `delay` expressed in block count.
pub fn create_program_with_gas_delayed(
    code_id: CodeId,
    salt: &[u8],
    payload: &[u8],
    gas_limit: u64,
    value: u128,
    delay: u32,
) -> Result<(MessageId, ActorId)> {
    let cid_value = HashWithValue {
        hash: code_id.0,
        value,
    };

    let mut res: LengthWithTwoHashes = Default::default();

    let salt_len = salt.len().try_into().map_err(|_| ExtError::SyscallUsage)?;

    let payload_len = payload
        .len()
        .try_into()
        .map_err(|_| ExtError::SyscallUsage)?;

    unsafe {
        gsys::gr_create_program_wgas(
            cid_value.as_ptr(),
            salt.as_ptr(),
            salt_len,
            payload.as_ptr(),
            payload_len,
            gas_limit,
            delay,
            res.as_mut_ptr(),
        )
    };
    SyscallError(res.length).into_result()?;

    Ok((MessageId(res.hash1), ActorId(res.hash2)))
}
