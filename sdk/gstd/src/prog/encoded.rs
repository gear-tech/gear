// Copyright (C) Gear Technologies Inc.
// SPDX-License-Identifier: GPL-3.0-or-later WITH Classpath-exception-2.0

use crate::{ActorId, CodeId, MessageId, util::with_optimized_encode};
use gcore::errors::Result;
use gstd_codegen::wait_create_program_for_reply;
use scale_info::scale::Encode;

/// Same as [`create_program_bytes`](super::create_program_bytes), but allows
/// initialize program with the encodable payload.
#[wait_create_program_for_reply]
pub fn create_program<E: Encode>(
    code_id: CodeId,
    salt: impl AsRef<[u8]>,
    payload: E,
    value: u128,
) -> Result<(MessageId, ActorId)> {
    with_optimized_encode(payload, |buffer| {
        super::create_program_bytes(code_id, salt, buffer, value)
    })
}

/// Same as [`create_program`], but creates a new program after the `delay`
/// expressed in block count.
pub fn create_program_delayed<E: Encode>(
    code_id: CodeId,
    salt: impl AsRef<[u8]>,
    payload: E,
    value: u128,
    delay: u32,
) -> Result<(MessageId, ActorId)> {
    with_optimized_encode(payload, |buffer| {
        super::create_program_bytes_delayed(code_id, salt, buffer, value, delay)
    })
}

/// Same as [`create_program`], but with an explicit gas limit.
#[wait_create_program_for_reply]
pub fn create_program_with_gas<E: Encode>(
    code_id: CodeId,
    salt: impl AsRef<[u8]>,
    payload: E,
    gas_limit: u64,
    value: u128,
) -> Result<(MessageId, ActorId)> {
    with_optimized_encode(payload, |buffer| {
        super::create_program_bytes_with_gas(code_id, salt, buffer, gas_limit, value)
    })
}

/// Same as [`create_program_with_gas`], but creates a new program after the
/// `delay` expressed in block count.
pub fn create_program_with_gas_delayed<E: Encode>(
    code_id: CodeId,
    salt: impl AsRef<[u8]>,
    payload: E,
    gas_limit: u64,
    value: u128,
    delay: u32,
) -> Result<(MessageId, ActorId)> {
    with_optimized_encode(payload, |buffer| {
        super::create_program_bytes_with_gas_delayed(code_id, salt, buffer, gas_limit, value, delay)
    })
}
