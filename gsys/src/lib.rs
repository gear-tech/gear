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

//! Declares gear protocol syscalls for WASM.

#![no_std]

/// Represents error code type.
pub type ErrorCode = u32;

/// Represents block number type.
pub type BlockNumber = u32;

/// Represents block count type.
pub type BlockCount = u32;

/// Represents block number type.
pub type BlockTimestamp = u64;

/// Represents byte type, which is a start of a buffer.
pub type BufferStart = u8;

/// Represents byte type, which is a start of a sized buffer.
///
/// This usually goes along with `Length` param.`
pub type SizedBufferStart = u8;

/// Represents gas type.
pub type Gas = u64;

/// Represents handle type.
pub type Handle = u32;

/// Represents hash type.
pub type Hash = [u8; 32];

/// Represents offset type.
pub type Offset = u32;

/// Represents length type.
pub type Length = u32;

/// Represents reply code type.
pub type ReplyCode = [u8; 4];

/// Represents signal code type.
pub type SignalCode = u32;

/// Represents value type.
pub type Value = u128;

/// Represents type defining concatenated block number with hash. 36 bytes.
#[repr(C, packed)]
#[derive(Default, Debug)]
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
#[derive(Default, Debug, Clone)]
pub struct HashWithValue {
    pub hash: Hash,
    pub value: Value,
}

impl HashWithValue {
    pub const fn as_ptr(&self) -> *const Self {
        self as _
    }
}

/// Represents type defining concatenated reply code with error code. 8 bytes.
#[repr(C, packed)]
#[derive(Default, Debug)]
pub struct ErrorWithReplyCode {
    pub error_code: ErrorCode,
    pub reply_code: ReplyCode,
}

impl ErrorWithReplyCode {
    pub fn as_mut_ptr(&mut self) -> *mut Self {
        self as _
    }
}

impl From<Result<ReplyCode, ErrorCode>> for ErrorWithReplyCode {
    fn from(result: Result<ReplyCode, ErrorCode>) -> Self {
        let mut res: Self = Default::default();

        match result {
            Ok(code) => res.reply_code = code,
            Err(length) => res.error_code = length,
        }

        res
    }
}

/// Represents type defining concatenated signal code with length. 8 bytes.
#[repr(C, packed)]
#[derive(Default, Debug)]
pub struct ErrorWithSignalCode {
    pub error_code: ErrorCode,
    pub signal_code: SignalCode,
}

impl ErrorWithSignalCode {
    pub fn as_mut_ptr(&mut self) -> *mut Self {
        self as _
    }
}

impl From<Result<SignalCode, ErrorCode>> for ErrorWithSignalCode {
    fn from(result: Result<SignalCode, ErrorCode>) -> Self {
        let mut res: Self = Default::default();

        match result {
            Ok(code) => res.signal_code = code,
            Err(code) => res.error_code = code,
        }

        res
    }
}

/// Represents type defining concatenated error code with gas. 12 bytes.
#[repr(C, packed)]
#[derive(Default, Debug)]
pub struct ErrorWithGas {
    pub error_code: ErrorCode,
    pub gas: Gas,
}

impl ErrorWithGas {
    pub fn as_mut_ptr(&mut self) -> *mut Self {
        self as _
    }
}

impl From<Result<Gas, ErrorCode>> for ErrorWithGas {
    fn from(result: Result<Gas, ErrorCode>) -> Self {
        let mut res: Self = Default::default();

        match result {
            Ok(gas) => res.gas = gas,
            Err(code) => res.error_code = code,
        }

        res
    }
}

/// Represents type defining concatenated length with handle. 8 bytes.
#[repr(C, packed)]
#[derive(Default, Debug)]
pub struct ErrorWithHandle {
    pub error_code: ErrorCode,
    pub handle: Handle,
}

impl ErrorWithHandle {
    pub fn as_mut_ptr(&mut self) -> *mut Self {
        self as _
    }
}

impl From<Result<Handle, ErrorCode>> for ErrorWithHandle {
    fn from(result: Result<Handle, ErrorCode>) -> Self {
        let mut res: Self = Default::default();

        match result {
            Ok(handle) => res.handle = handle,
            Err(code) => res.error_code = code,
        }

        res
    }
}

#[repr(C, packed)]
#[derive(Default, Debug)]
pub struct ErrorBytes([u8; size_of::<ErrorCode>()]);

impl From<Result<(), ErrorCode>> for ErrorBytes {
    fn from(value: Result<(), ErrorCode>) -> Self {
        Self(value.err().unwrap_or_default().to_le_bytes())
    }
}

/// Represents type defining concatenated hash with error code. 36 bytes.
#[repr(C, packed)]
#[derive(Default, Debug)]
pub struct ErrorWithHash {
    pub error_code: ErrorCode,
    pub hash: Hash,
}

impl ErrorWithHash {
    pub fn as_mut_ptr(&mut self) -> *mut Self {
        self as _
    }
}

impl<T: Into<[u8; 32]>> From<Result<T, ErrorCode>> for ErrorWithHash {
    fn from(result: Result<T, ErrorCode>) -> Self {
        let mut res: Self = Default::default();

        match result {
            Ok(v) => res.hash = v.into(),
            Err(code) => res.error_code = code,
        }

        res
    }
}

/// Represents type defining concatenated two hashes with error code. 68 bytes.
#[repr(C, packed)]
#[derive(Default, Debug)]
pub struct ErrorWithTwoHashes {
    pub error_code: ErrorCode,
    pub hash1: Hash,
    pub hash2: Hash,
}

impl ErrorWithTwoHashes {
    pub fn as_mut_ptr(&mut self) -> *mut Self {
        self as _
    }
}

impl<T1, T2> From<Result<(T1, T2), ErrorCode>> for ErrorWithTwoHashes
where
    T1: Into<[u8; 32]>,
    T2: Into<[u8; 32]>,
{
    fn from(result: Result<(T1, T2), ErrorCode>) -> Self {
        let mut res: Self = Default::default();

        match result {
            Ok((v1, v2)) => {
                res.hash1 = v1.into();
                res.hash2 = v2.into();
            }
            Err(code) => res.error_code = code,
        }

        res
    }
}

/// Represents type defining concatenated two hashes. 64 bytes.
#[repr(C, packed)]
#[derive(Default, Debug)]
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
#[derive(Default, Debug)]
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

/// Current version of execution settings.
///
/// Backend maintains backward compatibility with previous versions of execution
/// settings. This structure matches to the most recent version of execution
/// settings supported by backend.
#[repr(C, packed)]
#[derive(Debug, Clone, Copy)]
pub struct EnvVars {
    /// Current performance multiplier.
    pub performance_multiplier: Percent,
    /// Current value of existential deposit.
    pub existential_deposit: Value,
    /// Current value of mailbox threshold.
    pub mailbox_threshold: Gas,
    /// Current gas multiplier.
    pub gas_multiplier: GasMultiplier,
}

/// Basic struct for working with integer percentages allowing
/// values greater than 100.
// This is a "copy-paste" of the similar struct from the `core` crate
// which can't be used here due to its dependencies from codec and TypeInfo.
#[repr(C, packed)]
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct Percent(u32);

impl Percent {
    pub fn new(value: u32) -> Self {
        Self(value)
    }

    pub fn value(&self) -> u32 {
        self.0
    }
}

/// Type representing converter between gas and value.
// This is an FFI-friendly "copy-paste" of the similar enum from the `common` crate
// which can't be used here due to its dependencies from codec and TypeInfo as well
// as FFI.
#[repr(C, packed)]
#[derive(Debug, Clone, Copy)]
pub struct GasMultiplier {
    gas_per_value: Gas,
    value_per_gas: Value,
}

// TODO: make implementation safer, check overflows and division remaining (#4137).
impl GasMultiplier {
    /// Creates GasMultiplier with gas == value.
    pub const fn one() -> Self {
        Self {
            gas_per_value: 1,
            value_per_gas: 1,
        }
    }

    /// Creates GasMultiplier from gas per value multiplier.
    pub fn from_gas_per_value(gas_per_value: Gas) -> Self {
        if gas_per_value == 1 {
            Self::one()
        } else {
            Self {
                gas_per_value,
                value_per_gas: 0,
            }
        }
    }

    /// Creates GasMultiplier from value per gas multiplier.
    pub fn from_value_per_gas(value_per_gas: Value) -> Self {
        if value_per_gas == 1 {
            Self::one()
        } else {
            Self {
                gas_per_value: 0,
                value_per_gas,
            }
        }
    }

    /// Converts given gas amount into its value equivalent, rounding to upper, if Gas > Value.
    pub fn gas_to_value(&self, gas: Gas) -> Value {
        if self.value_per_gas != 0 {
            (gas as Value).saturating_mul(self.value_per_gas)
        } else {
            gas.div_ceil(self.gas_per_value) as _
        }
    }

    /// Converts given value amount into its gas equivalent, rounding to lower, if Gas > Value.
    pub fn value_to_gas(&self, value: Value) -> Gas {
        if self.gas_per_value != 0 {
            (value as Gas).saturating_mul(self.gas_per_value)
        } else {
            (value / self.value_per_gas) as _
        }
    }
}

macro_rules! syscalls {
    (
        $(
            $(#[$attrs:meta])*
            $vis:vis fn $symbol:ident(
                $($arg_name:ident: $arg_ty:ty),* $(,)?
            ) $(-> $ret_ty:ty)?;
        )*
    ) => {
        #[allow(improper_ctypes)]
        unsafe extern "C" {
            $(
                $(#[$attrs])*
                $vis fn $symbol($($arg_name: $arg_ty),*) $(-> $ret_ty)?;
            )*
        }

        #[cfg(not(target_arch = "wasm32"))]
        mod declarations {
            use $crate::*;

            $(
                #[unsafe(no_mangle)]
                $vis extern "C" fn $symbol($(_: $arg_ty),*) $(-> $ret_ty)? {
                    unimplemented!(concat!(
                        stringify!($symbol),
                        " syscall is only available for wasm32 architecture"
                    ))
                }
            )*
        }
    };
}

syscalls! {
    /// Infallible `gr_env_vars` get syscall.
    /// It leaves backend with unrecoverable error if incorrect version is passed.
    ///
    /// Arguments type:
    /// - `version`: `u32` defining version of vars to get.
    /// - `vars`: `mut ptr` for buffer to store requested version of vars.
    pub fn gr_env_vars(version: u32, vars: *mut BufferStart);

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
    /// - `err_mid_pid`: `mut ptr` for concatenated error code, message id
    ///   and program id.
    #[cfg(not(feature = "gearexe"))]
    pub fn gr_create_program_wgas(
        cid_value: *const HashWithValue,
        salt: *const SizedBufferStart,
        salt_len: Length,
        payload: *const SizedBufferStart,
        payload_len: Length,
        gas_limit: Gas,
        delay: BlockNumber,
        err_mid_pid: *mut ErrorWithTwoHashes,
    );

    /// Fallible `gr_create_program` send syscall.
    ///
    /// Arguments type:
    /// - `cid_value`: `const ptr` for concatenated code id and value.
    /// - `salt`: `const ptr` for the begging of the salt buffer.
    /// - `salt_len`: `u32` length of the salt buffer.
    /// - `payload`: `const ptr` for the begging of the payload buffer.
    /// - `payload_len`: `u32` length of the payload buffer.
    /// - `delay`: `u32` amount of blocks to delay.
    /// - `err_mid_pid`: `mut ptr` for concatenated error code, message id
    ///   and program id.
    pub fn gr_create_program(
        cid_value: *const HashWithValue,
        salt: *const SizedBufferStart,
        salt_len: Length,
        payload: *const SizedBufferStart,
        payload_len: Length,
        delay: BlockNumber,
        err_mid_pid: *mut ErrorWithTwoHashes,
    );

    /// Fallible `gr_reply_deposit` control syscall.
    ///
    /// Arguments type:
    /// - `message_id`: `const ptr` for message id.
    /// - `gas`: `u64` defining gas limit to deposit.
    /// - `err`: `mut ptr` for error code.
    #[cfg(not(feature = "gearexe"))]
    pub fn gr_reply_deposit(message_id: *const Hash, gas: Gas, err: *mut ErrorCode);

    /// Infallible `gr_debug` info syscall.
    ///
    /// Arguments type:
    /// - `payload`: `const ptr` for the begging of the payload buffer.
    /// - `len`: `u32` length of the payload buffer.
    pub fn gr_debug(payload: *const SizedBufferStart, len: Length);

    /// Infallible `gr_panic` control syscall.
    ///
    /// Stops the execution.
    ///
    /// Arguments type:
    /// - `payload`: `const ptr` for the begging of the payload buffer.
    /// - `len`: `u32` length of the payload buffer.
    pub fn gr_panic(payload: *const SizedBufferStart, len: Length) -> !;

    /// Infallible `gr_oom_panic` control syscall.
    pub fn gr_oom_panic() -> !;

    /// Fallible `gr_reply_code` get syscall.
    ///
    /// Arguments type:
    /// - `err_code`: `mut ptr` for concatenated error code and reply code.
    pub fn gr_reply_code(err_code: *mut ErrorWithReplyCode);

    /// Fallible `gr_signal_code` get syscall.
    ///
    /// Arguments type:
    /// - `err_code`: `mut ptr` for concatenated error code and signal code.
    #[cfg(not(feature = "gearexe"))]
    pub fn gr_signal_code(err_code: *mut ErrorWithSignalCode);

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

    /// Infallible `gr_program_id` get syscall.
    ///
    /// Arguments type:
    /// - `program_id`: `const ptr` for program id.
    pub fn gr_program_id(program_id: *mut Hash);

    /// Infallible `gr_random` calculate syscall.
    ///
    /// Arguments type:
    /// - `subject`: `const ptr` for the subject.
    /// - `bn_random`: `mut ptr` for concatenated block number with hash.
    pub fn gr_random(subject: *const Hash, bn_random: *mut BlockNumberWithHash);

    // TODO: issue #1859
    /// Fallible `gr_read` get syscall.
    ///
    /// Arguments type:
    /// - `at`: `u32` defining offset in the input buffer to read from.
    /// - `len`: `u32` length of the buffer to read.
    /// - `buffer`: `mut ptr` for buffer to store requested data.
    /// - `err`: `mut ptr` for `u32` error code.
    pub fn gr_read(at: Offset, len: Length, buffer: *mut SizedBufferStart, err: *mut ErrorCode);

    /// Fallible `gr_reply_commit_wgas` send syscall.
    ///
    /// Arguments type:
    /// - `gas_limit`: `u64` defining gas limit for sending.
    /// - `value`: `const ptr` for `u128` defining amount of value to apply.
    ///   Ignored if equals u32::MAX (use this for zero value for optimization).
    /// - `err_mid`: `mut ptr` for concatenated error code and message id.
    #[cfg(not(feature = "gearexe"))]
    pub fn gr_reply_commit_wgas(gas_limit: Gas, value: *const Value, err_mid: *mut ErrorWithHash);

    /// Fallible `gr_reply_commit` send syscall.
    ///
    /// Arguments type:
    /// - `value`: `const ptr` for `u128` defining amount of value to apply.
    ///   Ignored if equals u32::MAX (use this for zero value for optimization).
    /// - `err_mid`: `mut ptr` for concatenated error code and message id.
    pub fn gr_reply_commit(value: *const Value, err_mid: *mut ErrorWithHash);

    /// Fallible `gr_reply_push` send syscall.
    ///
    /// Arguments type:
    /// - `payload`: `const ptr` for the begging of the payload buffer.
    /// - `len`: `u32` length of the payload buffer.
    /// - `err`: `mut ptr` for error code.
    pub fn gr_reply_push(payload: *const SizedBufferStart, len: Length, err: *mut ErrorCode);

    /// Fallible `gr_reply_push_input` send syscall.
    ///
    /// Arguments type:
    /// - `offset`: `u32` defining start index of the input buffer to use.
    /// - `len`: `u32` defining slice length of the input buffer to use.
    /// - `err`: `mut ptr` for error code.
    pub fn gr_reply_push_input(offset: Offset, len: Length, err: *mut ErrorCode);

    /// Fallible `gr_reply_to` get syscall.
    ///
    /// Arguments type:
    /// - `err_mid`: `mut ptr` for concatenated error code and message id.
    pub fn gr_reply_to(err_mid: *mut ErrorWithHash);

    /// Fallible `gr_signal_from` get syscall.
    ///
    /// Arguments type:
    /// - `err_mid`: `mut ptr` for concatenated error code and message id.
    #[cfg(not(feature = "gearexe"))]
    pub fn gr_signal_from(err_mid: *mut ErrorWithHash);

    /// Fallible `gr_reply_input_wgas` send syscall.
    ///
    /// Arguments type:
    /// - `offset`: `u32` defining start index of the input buffer to use.
    /// - `len`: `u32` defining slice length of the input buffer to use.
    /// - `gas_limit`: `u64` defining gas limit for sending.
    /// - `value`: `const ptr` for `u128` defining amount of value to apply.
    ///   Ignored if equals u32::MAX (use this for zero value for optimization).
    /// - `err_mid`: `mut ptr` for concatenated error code and message id.
    #[cfg(not(feature = "gearexe"))]
    pub fn gr_reply_input_wgas(
        offset: Offset,
        len: Length,
        gas_limit: Gas,
        value: *const Value,
        err_mid: *mut ErrorWithHash,
    );

    /// Fallible `gr_reply_wgas` send syscall.
    ///
    /// Arguments type:
    /// - `payload`: `const ptr` for the begging of the payload buffer.
    /// - `len`: `u32` length of the payload buffer.
    /// - `gas_limit`: `u64` defining gas limit for sending.
    /// - `value`: `const ptr` for `u128` defining amount of value to apply.
    ///   Ignored if equals u32::MAX (use this for zero value for optimization).
    /// - `err_mid`: `mut ptr` for concatenated error code and message id.
    #[cfg(not(feature = "gearexe"))]
    pub fn gr_reply_wgas(
        payload: *const SizedBufferStart,
        len: Length,
        gas_limit: Gas,
        value: *const Value,
        err_mid: *mut ErrorWithHash,
    );

    /// Fallible `gr_reply` send syscall.
    ///
    /// Arguments type:
    /// - `payload`: `const ptr` for the begging of the payload buffer.
    /// - `len`: `u32` length of the payload buffer.
    /// - `value`: `const ptr` for `u128` defining amount of value to apply.
    ///   Ignored if equals u32::MAX (use this for zero value for optimization).
    /// - `err_mid`: `mut ptr` for concatenated error code and message id.
    pub fn gr_reply(
        payload: *const SizedBufferStart,
        len: Length,
        value: *const Value,
        err_mid: *mut ErrorWithHash,
    );

    /// Fallible `gr_reply_input` send syscall.
    ///
    /// Arguments type:
    /// - `offset`: `u32` defining start index of the input buffer to use.
    /// - `len`: `u32` defining slice length of the input buffer to use.
    /// - `value`: `const ptr` for `u128` defining amount of value to apply.
    ///   Ignored if equals u32::MAX (use this for zero value for optimization).
    /// - `err_mid`: `mut ptr` for concatenated error code and message id.
    pub fn gr_reply_input(
        offset: Offset,
        len: Length,
        value: *const Value,
        err_mid: *mut ErrorWithHash,
    );

    /// Fallible `gr_reservation_reply_commit` send syscall.
    ///
    /// Arguments type:
    /// - `rid_value`: `const ptr` for concatenated reservation id and value.
    /// - `err_mid`: `mut ptr` for concatenated error code and message id.
    #[cfg(not(feature = "gearexe"))]
    pub fn gr_reservation_reply_commit(
        rid_value: *const HashWithValue,
        err_mid: *mut ErrorWithHash,
    );

    /// Fallible `gr_reservation_reply` send syscall.
    ///
    /// Arguments type:
    /// - `rid_value`: `const ptr` for concatenated reservation id and value.
    /// - `payload`: `const ptr` for the begging of the payload buffer.
    /// - `len`: `u32` length of the payload buffer.
    /// - `err_mid`: `mut ptr` for concatenated error code and message id.
    #[cfg(not(feature = "gearexe"))]
    pub fn gr_reservation_reply(
        rid_value: *const HashWithValue,
        payload: *const SizedBufferStart,
        len: Length,
        err_mid: *mut ErrorWithHash,
    );

    /// Fallible `gr_reservation_send_commit` send syscall.
    ///
    /// Arguments type:
    /// - `handle`: `u32` defining handle of the message to commit.
    /// - `rid_pid_value`: `const ptr` for concatenated reservation id,
    ///   program id and value.
    /// - `delay`: `u32` amount of blocks to delay.
    /// - `err_mid`: `mut ptr` for concatenated error code and message id.
    #[cfg(not(feature = "gearexe"))]
    pub fn gr_reservation_send_commit(
        handle: Handle,
        rid_pid_value: *const TwoHashesWithValue,
        delay: BlockNumber,
        err_mid: *mut ErrorWithHash,
    );

    /// Fallible `gr_reservation_send` send syscall.
    ///
    /// Arguments type:
    /// - `rid_pid_value`: `const ptr` for concatenated reservation id,
    ///   program id and value.
    /// - `payload`: `const ptr` for the begging of the payload buffer.
    /// - `len`: `u32` length of the payload buffer.
    /// - `delay`: `u32` amount of blocks to delay.
    /// - `err_mid`: `mut ptr` for concatenated error code and message id.
    #[cfg(not(feature = "gearexe"))]
    pub fn gr_reservation_send(
        rid_pid_value: *const TwoHashesWithValue,
        payload: *const SizedBufferStart,
        len: Length,
        delay: BlockNumber,
        err_mid: *mut ErrorWithHash,
    );

    /// Fallible `gr_reserve_gas` control syscall.
    ///
    /// Arguments type:
    /// - `gas`: `u64` defining amount of gas to reserve.
    /// - `duration`: `u32` reservation duration.
    /// - `err_rid`: `mut ptr` for concatenated error code and reservation id.
    #[cfg(not(feature = "gearexe"))]
    pub fn gr_reserve_gas(gas: Gas, duration: BlockNumber, err_rid: *mut ErrorWithHash);

    /// Fallible `gr_send_commit_wgas` send syscall.
    ///
    /// Arguments type:
    /// - `handle`: `u32` defining handle of the message to commit.
    /// - `pid_value`: `const ptr` for concatenated program id and value.
    /// - `gas_limit`: `u64` defining gas limit for sending.
    /// - `delay`: `u32` amount of blocks to delay.
    /// - `err_mid`: `mut ptr` for concatenated error code and message id.
    #[cfg(not(feature = "gearexe"))]
    pub fn gr_send_commit_wgas(
        handle: Handle,
        pid_value: *const HashWithValue,
        gas_limit: Gas,
        delay: BlockNumber,
        err_mid: *mut ErrorWithHash,
    );

    /// Fallible `gr_send_commit` send syscall.
    ///
    /// Arguments type:
    /// - `handle`: `u32` defining handle of the message to commit.
    /// - `pid_value`: `const ptr` for concatenated program id and value.
    /// - `delay`: `u32` amount of blocks to delay.
    /// - `err_mid`: `mut ptr` for concatenated error code and message id.
    pub fn gr_send_commit(
        handle: Handle,
        pid_value: *const HashWithValue,
        delay: BlockNumber,
        err_mid: *mut ErrorWithHash,
    );

    /// Fallible `gr_send_init` send syscall.
    ///
    /// Arguments type:
    /// - `err_handle`: `mut ptr` for concatenated error code and handle.
    pub fn gr_send_init(err_handle: *mut ErrorWithHandle);

    /// Fallible `gr_send_push` send syscall.
    ///
    /// Arguments type:
    /// - `handle`: `u32` defining handle of the message to push into.
    /// - `payload`: `const ptr` for the begging of the payload buffer.
    /// - `len`: `u32` length of the payload buffer.
    /// - `err`: `mut ptr` for error code.
    pub fn gr_send_push(
        handle: Handle,
        payload: *const SizedBufferStart,
        len: Length,
        err: *mut ErrorCode,
    );

    /// Fallible `gr_send_push_input` send syscall.
    ///
    /// Arguments type:
    /// - `handle`: `u32` defining handle of the message to push into.
    /// - `offset`: `u32` defining start index of the input buffer to use.
    /// - `len`: `u32` defining slice length of the input buffer to use.
    /// - `err`: `mut ptr` for error code.
    pub fn gr_send_push_input(handle: Handle, offset: Offset, len: Length, err: *mut ErrorCode);

    /// Fallible `gr_send_input_wgas` send syscall.
    ///
    /// Arguments type:
    /// - `pid_value`: `const ptr` for concatenated program id and value.
    /// - `offset`: `u32` defining start index of the input buffer to use.
    /// - `len`: `u32` defining slice length of the input buffer to use.
    /// - `gas_limit`: `u64` defining gas limit for sending.
    /// - `delay`: `u32` amount of blocks to delay.
    /// - `err_mid`: `mut ptr` for concatenated error code and message id.
    #[cfg(not(feature = "gearexe"))]
    pub fn gr_send_input_wgas(
        pid_value: *const HashWithValue,
        offset: Offset,
        len: Length,
        gas_limit: Gas,
        delay: BlockNumber,
        err_mid: *mut ErrorWithHash,
    );

    /// Fallible `gr_send_wgas` send syscall.
    ///
    /// Arguments type:
    /// - `pid_value`: `const ptr` for concatenated program id and value.
    /// - `payload`: `const ptr` for the begging of the payload buffer.
    /// - `len`: `u32` length of the payload buffer.
    /// - `gas_limit`: `u64` defining gas limit for sending.
    /// - `delay`: `u32` amount of blocks to delay.
    /// - `err_mid`: `mut ptr` for concatenated error code and message id.
    #[cfg(not(feature = "gearexe"))]
    pub fn gr_send_wgas(
        pid_value: *const HashWithValue,
        payload: *const SizedBufferStart,
        len: Length,
        gas_limit: Gas,
        delay: BlockNumber,
        err_mid: *mut ErrorWithHash,
    );

    /// Fallible `gr_send` send syscall.
    ///
    /// Arguments type:
    /// - `pid_value`: `const ptr` for concatenated program id and value.
    /// - `payload`: `const ptr` for the begging of the payload buffer.
    /// - `len`: `u32` length of the payload buffer.
    /// - `delay`: `u32` amount of blocks to delay.
    /// - `err_mid`: `mut ptr` for concatenated error code and message id.
    pub fn gr_send(
        pid_value: *const HashWithValue,
        payload: *const SizedBufferStart,
        len: Length,
        delay: BlockNumber,
        err_mid: *mut ErrorWithHash,
    );

    /// Fallible `gr_send_input` send syscall.
    ///
    /// Arguments type:
    /// - `pid_value`: `const ptr` for concatenated program id and value.
    /// - `offset`: `u32` defining start index of the input buffer to use.
    /// - `len`: `u32` length of the payload buffer.
    /// - `delay`: `u32` amount of blocks to delay.
    /// - `err_mid`: `mut ptr` for concatenated error code and message id.
    pub fn gr_send_input(
        pid_value: *const HashWithValue,
        offset: Offset,
        len: Length,
        delay: BlockNumber,
        err_mid: *mut ErrorWithHash,
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
    /// - `err`: `mut ptr` for error code.
    #[cfg(not(feature = "gearexe"))]
    pub fn gr_system_reserve_gas(gas: Gas, err: *mut ErrorCode);

    /// Fallible `gr_unreserve_gas` control syscall.
    ///
    /// Arguments type:
    /// - `reservation_id`: `const ptr` for reservation id.
    /// - `err_unreserved`: `mut ptr` for concatenated error code and
    ///   unreserved gas amount.
    #[cfg(not(feature = "gearexe"))]
    pub fn gr_unreserve_gas(reservation_id: *const Hash, err_unreserved: *mut ErrorWithGas);

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
    /// - `err_mid`: `mut ptr` for error code.
    pub fn gr_wake(message_id: *const Hash, delay: BlockNumber, err: *mut ErrorCode);
}
