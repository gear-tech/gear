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

//! Declares gear protocol syscalls for WASM.

#![no_std]

use core::mem;

/// Represents block number type.
pub type BlockNumber = u32;

/// Represents block number type.
pub type BlockTimestamp = u64;

/// Represents byte type.
pub type BufferStart = u8;

/// Represents gas type.
pub type Gas = u64;

/// Represents handle type.
pub type Handle = u32;

/// Represents hash type.
pub type Hash = [u8; 32];

/// Represents index type.
pub type Index = u32;

/// Represents length type.
pub type Length = u32;

/// Represents status code type.
pub type StatusCode = i32;

/// Represents value type.
pub type Value = u128;

/// Represents type defining concatenated block number with hash. 36 bytes.
#[repr(C, packed)]
#[derive(Default)]
pub struct BlockNumberWithHash {
    pub bn: BlockNumber,
    pub hash: Hash,
}

impl BlockNumberWithHash {
    pub fn as_mut_ptr(&mut self) -> *mut Self {
        self as _
    }
}

/// Represents type defining concatenated hash with value. 48 bytes.
#[repr(C, packed)]
#[derive(Default)]
pub struct HashWithValue {
    pub hash: Hash,
    pub value: Value,
}

impl HashWithValue {
    pub const fn as_ptr(&self) -> *const Self {
        self as _
    }
}

/// Represents type defining concatenated status code with length. 8 bytes.
#[repr(C, packed)]
#[derive(Default)]
pub struct LengthWithCode {
    pub length: Length,
    pub code: StatusCode,
}

impl LengthWithCode {
    pub fn as_mut_ptr(&mut self) -> *mut Self {
        self as _
    }

    pub fn from(result: Result<StatusCode, Length>) -> Self {
        let mut res: Self = Default::default();

        match result {
            Ok(code) => res.code = code,
            Err(length) => res.length = length,
        }

        res
    }
}

impl From<Result<StatusCode, Length>> for LengthWithCode {
    fn from(result: Result<StatusCode, Length>) -> Self {
        let mut res: Self = Default::default();

        match result {
            Ok(code) => res.code = code,
            Err(length) => res.length = length,
        }

        res
    }
}

/// Represents type defining concatenated length with gas. 12 bytes.
#[repr(C, packed)]
#[derive(Default)]
pub struct LengthWithGas {
    pub length: Length,
    pub gas: Gas,
}

impl LengthWithGas {
    pub fn as_mut_ptr(&mut self) -> *mut Self {
        self as _
    }
}

impl From<Result<Gas, Length>> for LengthWithGas {
    fn from(result: Result<Gas, Length>) -> Self {
        let mut res: Self = Default::default();

        match result {
            Ok(gas) => res.gas = gas,
            Err(length) => res.length = length,
        }

        res
    }
}

/// Represents type defining concatenated length with handle. 8 bytes.
#[repr(C, packed)]
#[derive(Default)]
pub struct LengthWithHandle {
    pub length: Length,
    pub handle: Handle,
}

impl LengthWithHandle {
    pub fn as_mut_ptr(&mut self) -> *mut Self {
        self as _
    }
}

impl From<Result<Handle, Length>> for LengthWithHandle {
    fn from(result: Result<Handle, Length>) -> Self {
        let mut res: Self = Default::default();

        match result {
            Ok(handle) => res.handle = handle,
            Err(length) => res.length = length,
        }

        res
    }
}

#[repr(C, packed)]
#[derive(Default)]
pub struct LengthBytes([u8; mem::size_of::<Length>()]);

impl From<Result<(), Length>> for LengthBytes {
    fn from(value: Result<(), Length>) -> Self {
        Self(value.err().unwrap_or_default().to_le_bytes())
    }
}

/// Represents type defining concatenated hash with length. 36 bytes.
#[repr(C, packed)]
#[derive(Default)]
pub struct LengthWithHash {
    pub length: Length,
    pub hash: Hash,
}

impl LengthWithHash {
    pub fn as_mut_ptr(&mut self) -> *mut Self {
        self as _
    }
}

impl<T: Into<[u8; 32]>> From<Result<T, Length>> for LengthWithHash {
    fn from(result: Result<T, Length>) -> Self {
        let mut res: Self = Default::default();

        match result {
            Ok(v) => res.hash = v.into(),
            Err(length) => res.length = length,
        }

        res
    }
}

/// Represents type defining concatenated two hashes with length. 68 bytes.
#[repr(C, packed)]
#[derive(Default)]
pub struct LengthWithTwoHashes {
    pub length: Length,
    pub hash1: Hash,
    pub hash2: Hash,
}

impl LengthWithTwoHashes {
    pub fn as_mut_ptr(&mut self) -> *mut Self {
        self as _
    }
}

impl<T1, T2> From<Result<(T1, T2), Length>> for LengthWithTwoHashes
where
    T1: Into<[u8; 32]>,
    T2: Into<[u8; 32]>,
{
    fn from(result: Result<(T1, T2), Length>) -> Self {
        let mut res: Self = Default::default();

        match result {
            Ok((v1, v2)) => {
                res.hash1 = v1.into();
                res.hash2 = v2.into();
            }
            Err(length) => res.length = length,
        }

        res
    }
}

/// Represents type defining concatenated two hashes. 64 bytes.
#[repr(C, packed)]
#[derive(Default)]
pub struct TwoHashes {
    pub hash1: Hash,
    pub hash2: Hash,
}

impl TwoHashes {
    pub const fn as_ptr(&self) -> *const Self {
        self as _
    }
}

/// Represents type defining concatenated two hashes with value. 80 bytes.
#[repr(C, packed)]
#[derive(Default)]
pub struct TwoHashesWithValue {
    pub hash1: Hash,
    pub hash2: Hash,
    pub value: Value,
}

impl TwoHashesWithValue {
    pub const fn as_ptr(&self) -> *const Self {
        self as _
    }
}

#[allow(improper_ctypes)]
extern "C" {
    /// Infallible `gr_block_height` get syscall.
    ///
    /// Arguments type:
    /// - `height`: `mut ptr` for `u32`.
    pub fn gr_block_height(height: *mut BlockNumber);

    /// Infallible `gr_block_timestamp` get syscall.
    ///
    /// Arguments type:
    /// - `timestamp`: `mut ptr` for `u64`.
    pub fn gr_block_timestamp(timestamp: *mut BlockTimestamp);

    /// Fallible `gr_create_program_wgas` send syscall.
    ///
    /// Arguments type:
    /// - `cid_value`: `const ptr` for concatenated code id and value.
    /// - `salt`: `const ptr` for the begging of the salt buffer.
    /// - `salt_len`: `u32` length of the salt buffer.
    /// - `payload`: `const ptr` for the begging of the payload buffer.
    /// - `payload_len`: `u32` length of the payload buffer.
    /// - `gas_limit`: `u64` defining gas limit for sending.
    /// - `delay`: `u32` amount of blocks to delay.
    /// - `err_mid_pid`: `mut ptr` for concatenated error length, message id
    ///   and program id.
    pub fn gr_create_program_wgas(
        cid_value: *const HashWithValue,
        salt: *const BufferStart,
        salt_len: Length,
        payload: *const BufferStart,
        payload_len: Length,
        gas_limit: Gas,
        delay: BlockNumber,
        err_mid_pid: *mut LengthWithTwoHashes,
    );

    /// Fallible `gr_create_program` send syscall.
    ///
    /// Arguments type:
    /// - `cid_value`: `const ptr` for concatenated code id and value.
    /// - `salt`: `const ptr` for the begging of the salt buffer.
    /// - `salt_len`: `u32` length of the salt buffer.
    /// - `payload`: `const ptr` for the begging of the payload buffer.
    /// - `payload_len`: `u32` length of the payload buffer.
    /// - `gas_limit`: `u64` defining gas limit for sending.
    /// - `delay`: `u32` amount of blocks to delay.
    /// - `err_mid_pid`: `mut ptr` for concatenated error length, message id
    ///   and program id.
    pub fn gr_create_program(
        cid_value: *const HashWithValue,
        salt: *const BufferStart,
        salt_len: Length,
        payload: *const BufferStart,
        payload_len: Length,
        delay: BlockNumber,
        err_mid_pid: *mut LengthWithTwoHashes,
    );

    /// Infallible `gr_debug` info syscall.
    ///
    /// Arguments type:
    /// - `payload`: `const ptr` for the begging of the payload buffer.
    /// - `len`: `u32` length of the payload buffer.
    pub fn gr_debug(payload: *const BufferStart, len: Length);

    /// Infallible `gr_panic` control syscall.
    ///
    /// Stops the execution.
    ///
    /// Arguments type:
    /// - `payload`: `const ptr` for the begging of the payload buffer.
    /// - `len`: `u32` length of the payload buffer.
    pub fn gr_panic(payload: *const BufferStart, len: Length) -> !;

    /// Infallible `gr_oom_panic` control syscall.
    pub fn gr_oom_panic() -> !;

    // TODO: issue #1859
    /// Fallible `gr_error` get syscall.
    ///
    /// Arguments type:
    /// - `buf`: `mut ptr` for buffer to store previously occurred error.
    /// - `len`: `mut ptr` for `u32` current error length.
    pub fn gr_error(buf: *mut BufferStart, len: *mut Length);

    /// Fallible `gr_status_code` get syscall.
    ///
    /// Arguments type:
    /// - `err_code`: `mut ptr` for concatenated error length and status code.
    pub fn gr_status_code(err_code: *mut LengthWithCode);

    /// Infallible `gr_exit` control syscall.
    ///
    /// Arguments type:
    /// - `inheritor_id`: `const ptr` for program id.
    pub fn gr_exit(inheritor_id: *const Hash) -> !;

    /// Infallible `gr_gas_available` get syscall.
    ///
    /// Arguments type:
    /// - `gas`: `mut ptr` for `u64`.
    pub fn gr_gas_available(gas: *mut Gas);

    /// Infallible `gr_leave` control syscall.
    pub fn gr_leave() -> !;

    /// Infallible `gr_message_id` get syscall.
    ///
    /// Arguments type:
    /// - `message_id`: `const ptr` for message id.
    pub fn gr_message_id(message_id: *mut Hash);

    /// Infallible `gr_origin` get syscall.
    ///
    /// Arguments type:
    /// - `program_id`: `const ptr` for program id.
    pub fn gr_origin(program_id: *mut Hash);

    /// Infallible `gr_program_id` get syscall.
    ///
    /// Arguments type:
    /// - `program_id`: `const ptr` for program id.
    pub fn gr_program_id(program_id: *mut Hash);

    /// Infallible `gr_random` calculate syscall.
    ///
    /// Arguments type:
    /// - `subject`: `const ptr` for the begging of the payload buffer.
    /// - `bn_random`: `mut ptr` for concatenated block number with hash.
    pub fn gr_random(subject: *const BufferStart, bn_random: *mut BlockNumberWithHash);

    // TODO: issue #1859
    /// Fallible `gr_read` get syscall.
    ///
    /// Arguments type:
    /// - `at`: `u32` defining offset to read from.
    /// - `len`: `u32` length of the buffer to read.
    /// - `buffer`: `mut ptr` for buffer to store requested data.
    /// - `err`: `mut ptr` for `u32` error length.
    pub fn gr_read(at: Length, len: Length, buffer: *mut BufferStart, err: *mut Length);

    /// Fallible `gr_reply_commit_wgas` send syscall.
    ///
    /// Arguments type:
    /// - `gas_limit`: `u64` defining gas limit for sending.
    /// - `value`: `const ptr` for `u128` defining amount of value to apply.
    ///   Ignored if equals u32::MAX (use this for zero value for optimization).
    /// - `delay`: `u32` amount of blocks to delay.
    /// - `err_mid`: `mut ptr` for concatenated error length and message id.
    pub fn gr_reply_commit_wgas(
        gas_limit: Gas,
        value: *const Value,
        delay: BlockNumber,
        err_mid: *mut LengthWithHash,
    );

    /// Fallible `gr_reply_commit` send syscall.
    ///
    /// Arguments type:
    /// - `value`: `const ptr` for `u128` defining amount of value to apply.
    ///   Ignored if equals u32::MAX (use this for zero value for optimization).
    /// - `delay`: `u32` amount of blocks to delay.
    /// - `err_mid`: `mut ptr` for concatenated error length and message id.
    pub fn gr_reply_commit(value: *const Value, delay: BlockNumber, err_mid: *mut LengthWithHash);

    /// Fallible `gr_reply_push` send syscall.
    ///
    /// Arguments type:
    /// - `payload`: `const ptr` for the begging of the payload buffer.
    /// - `len`: `u32` length of the payload buffer.
    /// - `err`: `mut ptr` for error length.
    pub fn gr_reply_push(payload: *const BufferStart, len: Length, err: *mut Length);

    /// Fallible `gr_reply_push_input` send syscall.
    ///
    /// Arguments type:
    /// - `offset`: `u32` defining start index of the input buffer to use.
    /// - `len`: `u32` defining slice length of the input buffer to use.
    /// - `err`: `mut ptr` for error length.
    pub fn gr_reply_push_input(offset: Index, len: Length, err: *mut Length);

    /// Fallible `gr_reply_to` get syscall.
    ///
    /// Arguments type:
    /// - `err_mid`: `mut ptr` for concatenated error length and message id.
    pub fn gr_reply_to(err_mid: *mut LengthWithHash);

    /// Fallible `gr_signal_from` get syscall.
    ///
    /// Arguments type:
    /// - `err_mid`: `mut ptr` for concatenated error length and message id.
    pub fn gr_signal_from(err_mid: *mut LengthWithHash);

    /// Fallible `gr_reply_input_wgas` send syscall.
    ///
    /// Arguments type:
    /// - `offset`: `u32` defining start index of the input buffer to use.
    /// - `len`: `u32` defining slice length of the input buffer to use.
    /// - `gas_limit`: `u64` defining gas limit for sending.
    /// - `value`: `const ptr` for `u128` defining amount of value to apply.
    ///   Ignored if equals u32::MAX (use this for zero value for optimization).
    /// - `delay`: `u32` amount of blocks to delay.
    /// - `err_mid`: `mut ptr` for concatenated error length and message id.
    pub fn gr_reply_input_wgas(
        offset: Index,
        len: Length,
        gas_limit: Gas,
        value: *const Value,
        delay: BlockNumber,
        err_mid: *mut LengthWithHash,
    );

    /// Fallible `gr_reply_wgas` send syscall.
    ///
    /// Arguments type:
    /// - `payload`: `const ptr` for the begging of the payload buffer.
    /// - `len`: `u32` length of the payload buffer.
    /// - `gas_limit`: `u64` defining gas limit for sending.
    /// - `value`: `const ptr` for `u128` defining amount of value to apply.
    ///   Ignored if equals u32::MAX (use this for zero value for optimization).
    /// - `delay`: `u32` amount of blocks to delay.
    /// - `err_mid`: `mut ptr` for concatenated error length and message id.
    pub fn gr_reply_wgas(
        payload: *const BufferStart,
        len: Length,
        gas_limit: Gas,
        value: *const Value,
        delay: BlockNumber,
        err_mid: *mut LengthWithHash,
    );

    /// Fallible `gr_reply` send syscall.
    ///
    /// Arguments type:
    /// - `payload`: `const ptr` for the begging of the payload buffer.
    /// - `len`: `u32` length of the payload buffer.
    /// - `value`: `const ptr` for `u128` defining amount of value to apply.
    ///   Ignored if equals u32::MAX (use this for zero value for optimization).
    /// - `delay`: `u32` amount of blocks to delay.
    /// - `err_mid`: `mut ptr` for concatenated error length and message id.
    pub fn gr_reply(
        payload: *const BufferStart,
        len: Length,
        value: *const Value,
        delay: BlockNumber,
        err_mid: *mut LengthWithHash,
    );

    /// Fallible `gr_reply_input` send syscall.
    ///
    /// Arguments type:
    /// - `offset`: `u32` defining start index of the input buffer to use.
    /// - `len`: `u32` defining slice length of the input buffer to use.
    /// - `value`: `const ptr` for `u128` defining amount of value to apply.
    ///   Ignored if equals u32::MAX (use this for zero value for optimization).
    /// - `delay`: `u32` amount of blocks to delay.
    /// - `err_mid`: `mut ptr` for concatenated error length and message id.
    pub fn gr_reply_input(
        offset: Index,
        len: Length,
        value: *const Value,
        delay: BlockNumber,
        err_mid: *mut LengthWithHash,
    );

    /// Fallible `gr_reservation_reply_commit` send syscall.
    ///
    /// Arguments type:
    /// - `rid_value`: `const ptr` for concatenated reservation id and value.
    /// - `payload`: `const ptr` for the begging of the payload buffer.
    /// - `len`: `u32` length of the payload buffer.
    /// - `delay`: `u32` amount of blocks to delay.
    /// - `err_mid`: `mut ptr` for concatenated error length and message id.
    pub fn gr_reservation_reply_commit(
        rid_value: *const HashWithValue,
        delay: BlockNumber,
        err_mid: *mut LengthWithHash,
    );

    /// Fallible `gr_reservation_reply` send syscall.
    ///
    /// Arguments type:
    /// - `rid_value`: `const ptr` for concatenated reservation id and value.
    /// - `payload`: `const ptr` for the begging of the payload buffer.
    /// - `len`: `u32` length of the payload buffer.
    /// - `delay`: `u32` amount of blocks to delay.
    /// - `err_mid`: `mut ptr` for concatenated error length and message id.
    pub fn gr_reservation_reply(
        rid_value: *const HashWithValue,
        payload: *const BufferStart,
        len: Length,
        delay: BlockNumber,
        err_mid: *mut LengthWithHash,
    );

    /// Fallible `gr_reservation_send_commit` send syscall.
    ///
    /// Arguments type:
    /// - `handle`: `u32` defining handle of the message to commit.
    /// - `rid_pid_value`: `const ptr` for concatenated reservation id,
    ///   program id and value.
    /// - `delay`: `u32` amount of blocks to delay.
    /// - `err_mid`: `mut ptr` for concatenated error length and message id.
    pub fn gr_reservation_send_commit(
        handle: Handle,
        rid_pid_value: *const TwoHashesWithValue,
        delay: BlockNumber,
        err_mid: *mut LengthWithHash,
    );

    /// Fallible `gr_reservation_send` send syscall.
    ///
    /// Arguments type:
    /// - `rid_pid_value`: `const ptr` for concatenated reservation id,
    ///   program id and value.
    /// - `payload`: `const ptr` for the begging of the payload buffer.
    /// - `len`: `u32` length of the payload buffer.
    /// - `delay`: `u32` amount of blocks to delay.
    /// - `err_mid`: `mut ptr` for concatenated error length and message id.
    pub fn gr_reservation_send(
        rid_pid_value: *const TwoHashesWithValue,
        payload: *const BufferStart,
        len: Length,
        delay: BlockNumber,
        err_mid: *mut LengthWithHash,
    );

    /// Fallible `gr_reserve_gas` control syscall.
    ///
    /// Arguments type:
    /// - `gas`: `u64` defining amount of gas to reserve.
    /// - `delay`: `u32` amount of blocks to delay.
    /// - `err_rid`: `mut ptr` for concatenated error length and reservation id.
    pub fn gr_reserve_gas(gas: Gas, duration: BlockNumber, err_rid: *mut LengthWithHash);

    /// Fallible `gr_send_commit_wgas` send syscall.
    ///
    /// Arguments type:
    /// - `handle`: `u32` defining handle of the message to commit.
    /// - `pid_value`: `const ptr` for concatenated program id and value.
    /// - `gas_limit`: `u64` defining gas limit for sending.
    /// - `delay`: `u32` amount of blocks to delay.
    /// - `err_mid`: `mut ptr` for concatenated error length and message id.
    pub fn gr_send_commit_wgas(
        handle: Handle,
        pid_value: *const HashWithValue,
        gas_limit: Gas,
        delay: BlockNumber,
        err_mid: *mut LengthWithHash,
    );

    /// Fallible `gr_send_commit` send syscall.
    ///
    /// Arguments type:
    /// - `handle`: `u32` defining handle of the message to commit.
    /// - `pid_value`: `const ptr` for concatenated program id and value.
    /// - `delay`: `u32` amount of blocks to delay.
    /// - `err_mid`: `mut ptr` for concatenated error length and message id.
    pub fn gr_send_commit(
        handle: Handle,
        pid_value: *const HashWithValue,
        delay: BlockNumber,
        err_mid: *mut LengthWithHash,
    );

    /// Fallible `gr_send_init` send syscall.
    ///
    /// Arguments type:
    /// - `err_handle`: `mut ptr` for concatenated error length and handle.
    pub fn gr_send_init(err_handle: *mut LengthWithHandle);

    /// Fallible `gr_send_push` send syscall.
    ///
    /// Arguments type:
    /// - `handle`: `u32` defining handle of the message to push into.
    /// - `payload`: `const ptr` for the begging of the payload buffer.
    /// - `len`: `u32` length of the payload buffer.
    /// - `err`: `mut ptr` for error length.
    pub fn gr_send_push(handle: Handle, payload: *const BufferStart, len: Length, err: *mut Length);

    /// Fallible `gr_send_push_input` send syscall.
    ///
    /// Arguments type:
    /// - `handle`: `u32` defining handle of the message to push into.
    /// - `offset`: `u32` defining start index of the input buffer to use.
    /// - `len`: `u32` defining slice length of the input buffer to use.
    /// - `err`: `mut ptr` for error length.
    pub fn gr_send_push_input(handle: Handle, offset: Index, len: Length, err: *mut Length);

    /// Fallible `gr_send_input_wgas` send syscall.
    ///
    /// Arguments type:
    /// - `pid_value`: `const ptr` for concatenated program id and value.
    /// - `offset`: `u32` defining start index of the input buffer to use.
    /// - `len`: `u32` defining slice length of the input buffer to use.
    /// - `gas_limit`: `u64` defining gas limit for sending.
    /// - `delay`: `u32` amount of blocks to delay.
    /// - `err_mid`: `mut ptr` for concatenated error length and message id.
    pub fn gr_send_input_wgas(
        pid_value: *const HashWithValue,
        offset: Index,
        len: Length,
        gas_limit: Gas,
        delay: BlockNumber,
        err_mid: *mut LengthWithHash,
    );

    /// Fallible `gr_send_wgas` send syscall.
    ///
    /// Arguments type:
    /// - `pid_value`: `const ptr` for concatenated program id and value.
    /// - `payload`: `const ptr` for the begging of the payload buffer.
    /// - `len`: `u32` length of the payload buffer.
    /// - `gas_limit`: `u64` defining gas limit for sending.
    /// - `delay`: `u32` amount of blocks to delay.
    /// - `err_mid`: `mut ptr` for concatenated error length and message id.
    pub fn gr_send_wgas(
        pid_value: *const HashWithValue,
        payload: *const BufferStart,
        len: Length,
        gas_limit: Gas,
        delay: BlockNumber,
        err_mid: *mut LengthWithHash,
    );

    /// Fallible `gr_send` send syscall.
    ///
    /// Arguments type:
    /// - `pid_value`: `const ptr` for concatenated program id and value.
    /// - `payload`: `const ptr` for the begging of the payload buffer.
    /// - `len`: `u32` length of the payload buffer.
    /// - `delay`: `u32` amount of blocks to delay.
    /// - `err_mid`: `mut ptr` for concatenated error length and message id.
    pub fn gr_send(
        pid_value: *const HashWithValue,
        payload: *const BufferStart,
        len: Length,
        delay: BlockNumber,
        err_mid: *mut LengthWithHash,
    );

    /// Fallible `gr_send_input` send syscall.
    ///
    /// Arguments type:
    /// - `pid_value`: `const ptr` for concatenated program id and value.
    /// - `payload`: `const ptr` for the begging of the payload buffer.
    /// - `len`: `u32` length of the payload buffer.
    /// - `delay`: `u32` amount of blocks to delay.
    /// - `err_mid`: `mut ptr` for concatenated error length and message id.
    pub fn gr_send_input(
        pid_value: *const HashWithValue,
        offset: Index,
        len: Length,
        delay: BlockNumber,
        err_mid: *mut LengthWithHash,
    );

    /// Infallible `gr_size` get syscall.
    ///
    /// Arguments type:
    /// - `length`: `mut ptr` for length of the incoming payload.
    pub fn gr_size(length: *mut Length);

    /// Infallible `gr_source` get syscall.
    ///
    /// Arguments type:
    /// - `program_id`: `const ptr` for program id.
    pub fn gr_source(program_id: *mut Hash);

    /// Fallible `gr_system_reserve_gas` control syscall.
    ///
    /// Arguments type:
    /// - `gas`: `u64` defining amount of gas to reserve.
    /// - `err`: `mut ptr` for error length.
    pub fn gr_system_reserve_gas(gas: Gas, err: *mut Length);

    /// Fallible `gr_unreserve_gas` control syscall.
    ///
    /// Arguments type:
    /// - `reservation_id`: `const ptr` for reservation id.
    /// - `err_unreserved`: `mut ptr` for concatenated error length and
    ///   unreserved gas amount.
    pub fn gr_unreserve_gas(reservation_id: *const Hash, err_unreserved: *mut LengthWithGas);

    /// Infallible `gr_value_available` get syscall.
    ///
    /// Arguments type:
    /// - `value`: `mut ptr` for total value of the program.
    pub fn gr_value_available(value: *mut Value);

    /// Infallible `gr_value` get syscall.
    ///
    /// Arguments type:
    /// - `value`: `mut ptr` for incoming value of the message.
    pub fn gr_value(value: *mut Value);

    /// Infallible `gr_wait_for` control syscall.
    ///
    /// Arguments type:
    /// - `duration`: `u32` defining amount of blocks to wait.
    pub fn gr_wait_for(duration: BlockNumber) -> !;

    /// Infallible `gr_wait_up_to` control syscall.
    ///
    /// Arguments type:
    /// - `duration`: `u32` defining amount of blocks to wait.
    pub fn gr_wait_up_to(duration: BlockNumber) -> !;

    /// Infallible `gr_wait` control syscall.
    pub fn gr_wait() -> !;

    /// Fallible `gr_wake` control syscall.
    ///
    /// Arguments type:
    /// - `message_id`: `const ptr` for message id.
    /// - `delay`: `u32` amount of blocks to delay.
    /// - `err_mid`: `mut ptr` for error length.
    pub fn gr_wake(message_id: *const Hash, delay: BlockNumber, err: *mut Length);
}
