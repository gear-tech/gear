// This file is part of Gear.
//
// Copyright (C) 2024-2025 Gear Technologies Inc.
// SPDX-License-Identifier: GPL-3.0-or-later WITH Classpath-exception-2.0
//
// This program is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.
//
// This program is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE. See the
// GNU General Public License for more details.
//
// You should have received a copy of the GNU General Public License
// along with this program. If not, see <https://www.gnu.org/licenses/>.

use alloc::{boxed::Box, vec::Vec};
use ethexe_runtime_common::pack_u32_to_i64;
use parity_scale_codec::{Decode, Encode};

mod instrument;
mod run;

#[cfg(target_arch = "wasm32")]
#[unsafe(no_mangle)]
extern "C" fn instrument_code(code_ptr: i32, code_len: i32) -> i64 {
    _instrument_code(code_ptr, code_len)
}

#[cfg_attr(not(target_arch = "wasm32"), allow(unused))]
fn _instrument_code(original_code_ptr: i32, original_code_len: i32) -> i64 {
    let code = get_vec(original_code_ptr, original_code_len);
    let res = instrument::instrument_code(code);
    return_val(res)
}

#[cfg(target_arch = "wasm32")]
#[unsafe(no_mangle)]
extern "C" fn run(arg_ptr: i32, arg_len: i32) -> i64 {
    _run(arg_ptr, arg_len)
}

#[cfg_attr(not(target_arch = "wasm32"), allow(unused))]
fn _run(arg_ptr: i32, arg_len: i32) -> i64 {
    let (program_id, state_root, maybe_instrumented_code, maybe_code_metadata, gas_allowance) =
        Decode::decode(&mut get_slice(arg_ptr, arg_len)).unwrap();

    let (program_journals, gas_spent) = run::run(
        program_id,
        state_root,
        maybe_instrumented_code,
        maybe_code_metadata,
        gas_allowance,
    );

    // Split to chunks to prevent alloc limit (32MiB)
    let res: Vec<_> = program_journals
        .into_iter()
        .flat_map(|(journal, origin, call_reply)| {
            let chunks = journal.encoded_size().div_ceil(32 * 1024 * 1024);
            let chunk_size = journal.len().div_ceil(chunks);

            let chunked_journal: Vec<_> = journal
                .chunks(chunk_size)
                .map(|chunk| (chunk, origin, call_reply))
                .map(return_val)
                .collect();

            chunked_journal
        })
        .collect();

    return_val((res, gas_spent))
}

fn get_vec(ptr: i32, len: i32) -> Vec<u8> {
    unsafe { Vec::from_raw_parts(ptr as _, len as usize, len as usize) }
}

fn get_slice<'a>(ptr: i32, len: i32) -> &'a [u8] {
    unsafe { core::slice::from_raw_parts(ptr as _, len as usize) }
}

fn return_val(val: impl Encode) -> i64 {
    let encoded = val.encode();
    let len = encoded.len() as i32;
    let ptr = Box::leak(Box::new(encoded)).as_ptr() as i32;

    pack_u32_to_i64(ptr as u32, len as u32)
}
