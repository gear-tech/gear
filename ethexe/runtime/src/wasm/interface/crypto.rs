// Copyright (C) Gear Technologies Inc.
// SPDX-License-Identifier: GPL-3.0-or-later WITH Classpath-exception-2.0

use crate::wasm::interface;
use alloc::{vec, vec::Vec};
use ethexe_runtime_common::pack_u32_to_i64;
use gsys::CryptoOp;

interface::declare! {
    pub(super) fn ext_crypto_version_1(op: u32, input_ptr_len: i64, output_ptr: i32) -> i32;
}

/// Forward a crypto operation to the host. The host writes exactly
/// [`CryptoOp::output_len`] bytes into `output` and returns 0 on success,
/// non-zero on malformed input.
pub fn crypto(op: CryptoOp, input: &[u8]) -> Option<Vec<u8>> {
    let mut output = vec![0u8; op.output_len() as usize];
    let input_ptr_len = pack_u32_to_i64(input.as_ptr() as u32, input.len() as u32);

    let res =
        unsafe { sys::ext_crypto_version_1(op as u32, input_ptr_len, output.as_mut_ptr() as i32) };

    (res == 0).then_some(output)
}
