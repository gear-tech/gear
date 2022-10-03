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

//! Program creation API for Gear programs.

use gear_core_errors::ExtError;

use crate::{error::Result, ActorId, CodeId, MessageId};

mod sys {
    use crate::error::SyscallError;

    extern "C" {
        #[allow(improper_ctypes)]
        pub fn gr_create_program(
            code_id_ptr: *const [u8; 32],
            salt_ptr: *const u8,
            salt_len: u32,
            payload_ptr: *const u8,
            payload_len: u32,
            value_ptr: *const u128,
            delay: u32,
            message_id_ptr: *mut [u8; 32],
            program_id_ptr: *mut [u8; 32],
        ) -> SyscallError;

        #[allow(improper_ctypes)]
        pub fn gr_create_program_wgas(
            code_id_ptr: *const [u8; 32],
            salt_ptr: *const u8,
            salt_len: u32,
            payload_ptr: *const u8,
            payload_len: u32,
            gas_limit: u64,
            value_ptr: *const u128,
            delay: u32,
            message_id_ptr: *mut [u8; 32],
            program_id_ptr: *mut [u8; 32],
        ) -> SyscallError;
    }
}

/// Same as [`create_program_with_gas`], but without explicit gas limit.
pub fn create_program(
    code_id: CodeId,
    salt: &[u8],
    payload: &[u8],
    value: u128,
) -> Result<(MessageId, ActorId)> {
    create_program_delayed(code_id, salt, payload, value, 0)
}

/// Same as [`create_program`], but sends delayed.
pub fn create_program_delayed(
    code_id: CodeId,
    salt: &[u8],
    payload: &[u8],
    value: u128,
    delay: u32,
) -> Result<(MessageId, ActorId)> {
    let mut message_id = MessageId::default();
    let mut program_id = ActorId::default();

    let salt_len = salt.len().try_into().map_err(|_| ExtError::SyscallUsage)?;

    let payload_len = payload
        .len()
        .try_into()
        .map_err(|_| ExtError::SyscallUsage)?;

    unsafe {
        sys::gr_create_program(
            code_id.as_ptr(),
            salt.as_ptr(),
            salt_len,
            payload.as_ptr(),
            payload_len,
            value.to_le_bytes().as_ptr() as *const u128,
            delay,
            message_id.as_mut_ptr(),
            program_id.as_mut_ptr(),
        )
        .into_result()?
    }

    Ok((message_id, program_id))
}

/// Creates a new program and returns its address, with gas limit.
///
/// The function creates a program initialization message and, as
/// any message send function in the crate, this one requires common additional
/// data for message execution, such as:
/// 1. `payload` that can be used in `init` function of the newly deployed
/// "child" program; 2. `gas_limit`, provided for the program initialization;
/// 3. `value`, sent with the message.
/// Code of newly creating program must be represented as blake2b hash
/// (`code_id` parameter).
///
/// # Examples
///
/// In order to generate an address for a new program `salt` must be provided.
/// Control of salt uniqueness is fully on a program developer side.
///
/// Basically we can use "automatic" salt generation ("nonce"):
/// ```
/// use gcore::{prog, CodeHash};
///
/// static mut NONCE: i32 = 0;
///
/// fn increase() {
///     unsafe {
///         NONCE += 1;
///     }
/// }
///
/// fn get() -> i32 {
///     unsafe { NONCE }
/// }
///
/// unsafe extern "C" fn handle() {
///     let submitted_code: CodeHash =
///         hex_literal::hex!("abf3746e72a6e8740bd9e12b879fbdd59e052cb390f116454e9116c22021ae4a")
///             .into();
///     let new_program_id =
///         prog::create_program_with_gas(submitted_code, &get().to_le_bytes(), b"", 10_000, 0)
///             .unwrap();
/// }
/// ```
/// Another case for salt is to receive it as an input:
/// ```
/// use gcore::{msg, prog};
/// # use gcore::CodeHash;
///
/// unsafe extern "C" fn handle() {
///     # let submitted_code: CodeHash = hex_literal::hex!("abf3746e72a6e8740bd9e12b879fbdd59e052cb390f116454e9116c22021ae4a").into();
///     let mut salt = vec![0u8; msg::size()];
///     msg::load(&mut salt[..]);
///     let new_program_id = prog::create_program_with_gas(submitted_code, &salt, b"", 10_000, 0).unwrap();
/// }
/// ```
///
/// What's more, messages can be sent to a new program:
/// ```
/// use gcore::{msg, prog};
/// # use gcore::CodeHash;
///
/// unsafe extern "C" fn handle() {
///     # let submitted_code: CodeHash = hex_literal::hex!("abf3746e72a6e8740bd9e12b879fbdd59e052cb390f116454e9116c22021ae4a").into();
///     # let mut salt = vec![0u8; msg::size()];
///     # msg::load(&mut salt[..]);
///     let new_program_id = prog::create_program_with_gas(submitted_code, &salt, b"", 10_000, 0).unwrap();
///     msg::send_with_gas(new_program_id, b"payload for a new program", 10_000, 0).unwrap();
/// }
/// ```
pub fn create_program_with_gas(
    code_id: CodeId,
    salt: &[u8],
    payload: &[u8],
    gas_limit: u64,
    value: u128,
) -> Result<(MessageId, ActorId)> {
    create_program_with_gas_delayed(code_id, salt, payload, gas_limit, value, 0)
}

/// Same as [`create_program_with_gas`], but sends delayed.
pub fn create_program_with_gas_delayed(
    code_id: CodeId,
    salt: &[u8],
    payload: &[u8],
    gas_limit: u64,
    value: u128,
    delay: u32,
) -> Result<(MessageId, ActorId)> {
    let mut message_id = MessageId::default();
    let mut program_id = ActorId::default();

    let salt_len = salt.len().try_into().map_err(|_| ExtError::SyscallUsage)?;

    let payload_len = payload
        .len()
        .try_into()
        .map_err(|_| ExtError::SyscallUsage)?;

    unsafe {
        sys::gr_create_program_wgas(
            code_id.as_ptr(),
            salt.as_ptr(),
            salt_len,
            payload.as_ptr(),
            payload_len,
            gas_limit,
            value.to_le_bytes().as_ptr() as *const u128,
            delay,
            message_id.as_mut_ptr(),
            program_id.as_mut_ptr(),
        )
        .into_result()?
    }

    Ok((message_id, program_id))
}
