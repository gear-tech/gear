// This file is part of Gear.
//
// Copyright (C) 2024-2026 Gear Technologies Inc.
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

use crate::host::api::MemoryWrap;
use ethexe_runtime_common::unpack_i64_to_u32;
use sp_core::{
    crypto::Pair as PairTrait,
    sr25519::{Pair as SrPair, Public, Signature},
};
use sp_wasm_interface::StoreData;
use wasmtime::{Caller, Linker};

pub fn link(linker: &mut Linker<StoreData>) -> Result<(), wasmtime::Error> {
    linker.func_wrap("env", "ext_sr25519_verify_v1", sr25519_verify)?;

    Ok(())
}

fn sr25519_verify(
    caller: Caller<'_, StoreData>,
    pk_ptr: i32,
    msg_packed: i64,
    sig_ptr: i32,
) -> i32 {
    log::trace!(target: "host_call", "sr25519_verify(pk_ptr={pk_ptr:?}, msg_packed={msg_packed:?}, sig_ptr={sig_ptr:?})");

    let memory = MemoryWrap(caller.data().memory());

    let pk_bytes = memory.slice(&caller, pk_ptr as usize, 32);
    let pk_array: [u8; 32] = match pk_bytes.try_into() {
        Ok(a) => a,
        Err(_) => return 0,
    };

    let (msg_ptr, msg_len) = unpack_i64_to_u32(msg_packed);
    let msg = memory.slice(&caller, msg_ptr as usize, msg_len as usize);

    let sig_bytes = memory.slice(&caller, sig_ptr as usize, 64);
    let sig_array: [u8; 64] = match sig_bytes.try_into() {
        Ok(a) => a,
        Err(_) => return 0,
    };

    let pk = Public::from_raw(pk_array);
    let sig = Signature::from_raw(sig_array);

    let ok = <SrPair as PairTrait>::verify(&sig, msg, &pk);

    log::trace!(target: "host_call", "sr25519_verify(..) -> {ok:?}");

    i32::from(ok)
}
