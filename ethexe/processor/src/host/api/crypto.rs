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
use gear_core::crypto::is_low_s;
use sp_core::crypto::Pair as PairTrait;
use sp_wasm_interface::StoreData;
use wasmtime::{Caller, Linker};

pub fn link(linker: &mut Linker<StoreData>) -> Result<(), wasmtime::Error> {
    linker.func_wrap("env", "ext_sr25519_verify_v1", sr25519_verify)?;
    linker.func_wrap("env", "ext_ed25519_verify_v1", ed25519_verify)?;
    linker.func_wrap("env", "ext_secp256k1_verify_v1", secp256k1_verify)?;
    linker.func_wrap("env", "ext_secp256k1_recover_v1", secp256k1_recover)?;

    Ok(())
}

/// Read a fixed-size byte array from guest memory, or return None if
/// the slice is the wrong length.
fn read_fixed<const N: usize>(
    memory: &MemoryWrap,
    caller: &Caller<'_, StoreData>,
    ptr: i32,
) -> Option<[u8; N]> {
    memory.slice(caller, ptr as usize, N).try_into().ok()
}

fn sr25519_verify(
    caller: Caller<'_, StoreData>,
    pk_ptr: i32,
    ctx_packed: i64,
    msg_packed: i64,
    sig_ptr: i32,
) -> i32 {
    log::trace!(
        target: "host_call",
        "sr25519_verify(pk_ptr={pk_ptr:?}, ctx_packed={ctx_packed:?}, msg_packed={msg_packed:?}, sig_ptr={sig_ptr:?})"
    );

    let memory = MemoryWrap(caller.data().memory());

    let pk_array: [u8; 32] = match read_fixed(&memory, &caller, pk_ptr) {
        Some(a) => a,
        None => return 0,
    };
    let sig_array: [u8; 64] = match read_fixed(&memory, &caller, sig_ptr) {
        Some(a) => a,
        None => return 0,
    };

    let (ctx_ptr, ctx_len) = unpack_i64_to_u32(ctx_packed);
    let ctx = memory.slice(&caller, ctx_ptr as usize, ctx_len as usize);
    let (msg_ptr, msg_len) = unpack_i64_to_u32(msg_packed);
    let msg = memory.slice(&caller, msg_ptr as usize, msg_len as usize);

    // Use schnorrkel directly so the caller can pass any simple
    // signing context. Mirrors the Vara impl at
    // `core/processor/src/ext.rs::sr25519_verify`.
    let Ok(public) = schnorrkel::PublicKey::from_bytes(&pk_array) else {
        return 0;
    };
    let Ok(signature) = schnorrkel::Signature::from_bytes(&sig_array) else {
        return 0;
    };
    let ok = public.verify_simple(ctx, msg, &signature).is_ok();

    log::trace!(target: "host_call", "sr25519_verify(..) -> {ok:?}");

    i32::from(ok)
}

fn ed25519_verify(
    caller: Caller<'_, StoreData>,
    pk_ptr: i32,
    msg_packed: i64,
    sig_ptr: i32,
) -> i32 {
    use sp_core::ed25519::{Pair, Public, Signature};

    log::trace!(target: "host_call", "ed25519_verify(pk_ptr={pk_ptr:?}, msg_packed={msg_packed:?}, sig_ptr={sig_ptr:?})");

    let memory = MemoryWrap(caller.data().memory());

    let pk_array: [u8; 32] = match read_fixed(&memory, &caller, pk_ptr) {
        Some(a) => a,
        None => return 0,
    };
    let sig_array: [u8; 64] = match read_fixed(&memory, &caller, sig_ptr) {
        Some(a) => a,
        None => return 0,
    };

    let (msg_ptr, msg_len) = unpack_i64_to_u32(msg_packed);
    let msg = memory.slice(&caller, msg_ptr as usize, msg_len as usize);

    let pk = Public::from_raw(pk_array);
    let sig = Signature::from_raw(sig_array);
    let ok = <Pair as PairTrait>::verify(&sig, msg, &pk);

    log::trace!(target: "host_call", "ed25519_verify(..) -> {ok:?}");

    i32::from(ok)
}

fn secp256k1_verify(
    caller: Caller<'_, StoreData>,
    msg_hash_ptr: i32,
    sig_ptr: i32,
    pk_ptr: i32,
    malleability_flag: i32,
) -> i32 {
    use sp_core::ecdsa::{Pair, Public, Signature};

    log::trace!(
        target: "host_call",
        "secp256k1_verify(msg_hash_ptr={msg_hash_ptr:?}, sig_ptr={sig_ptr:?}, pk_ptr={pk_ptr:?}, malleability_flag={malleability_flag:?})"
    );

    let memory = MemoryWrap(caller.data().memory());

    let msg_hash: [u8; 32] = match read_fixed(&memory, &caller, msg_hash_ptr) {
        Some(a) => a,
        None => return 0,
    };
    let sig_array: [u8; 65] = match read_fixed(&memory, &caller, sig_ptr) {
        Some(a) => a,
        None => return 0,
    };
    let pk_array: [u8; 33] = match read_fixed(&memory, &caller, pk_ptr) {
        Some(a) => a,
        None => return 0,
    };

    // Shared low-s policy with the Vara path (gear-core::crypto).
    // Both networks give identical answers for the same (sig, flag).
    let flag = malleability_flag as u32;
    if flag > 1 {
        return 0;
    }
    if flag == 1 && !is_low_s(&sig_array) {
        return 0;
    }

    let pk = Public::from_raw(pk_array);
    let sig = Signature::from_raw(sig_array);
    let ok = <Pair>::verify_prehashed(&sig, &msg_hash, &pk);

    log::trace!(target: "host_call", "secp256k1_verify(..) -> {ok:?}");

    i32::from(ok)
}

/// Returns 0 on success, 1 on recovery failure, 3 for unknown
/// malleability flag values. Writes the 65-byte SEC1 uncompressed
/// pubkey (`0x04 || x || y`) into `out_pk_ptr` on success; zero-fills
/// that buffer in every failure case so callers see a defined output.
///
/// Error codes must match `core/backend/src/funcs.rs::secp256k1_recover`'s
/// Vara wrapper byte-for-byte — a contract branching on err code must
/// get the same value on both networks.
fn secp256k1_recover(
    mut caller: Caller<'_, StoreData>,
    msg_hash_ptr: i32,
    sig_ptr: i32,
    malleability_flag: i32,
    out_pk_ptr: i32,
) -> i32 {
    log::trace!(
        target: "host_call",
        "secp256k1_recover(msg_hash_ptr={msg_hash_ptr:?}, sig_ptr={sig_ptr:?}, malleability_flag={malleability_flag:?}, out_pk_ptr={out_pk_ptr:?})"
    );

    let memory = MemoryWrap(caller.data().memory());

    let flag = malleability_flag as u32;
    // Unknown flag — bail before any crypto work. Matches the Vara
    // wrapper at core/backend/src/funcs.rs::secp256k1_recover which
    // also returns err = 3 on this path. Consistency across networks
    // is a hard requirement — the same (sig, flag) must fail the same
    // way everywhere.
    if flag > 1 {
        memory
            .slice_mut(&mut caller, out_pk_ptr as usize, 65)
            .copy_from_slice(&[0u8; 65]);
        return 3;
    }

    let msg_hash: [u8; 32] = match read_fixed(&memory, &caller, msg_hash_ptr) {
        Some(a) => a,
        None => {
            memory
                .slice_mut(&mut caller, out_pk_ptr as usize, 65)
                .copy_from_slice(&[0u8; 65]);
            return 1;
        }
    };
    let sig_array: [u8; 65] = match read_fixed(&memory, &caller, sig_ptr) {
        Some(a) => a,
        None => {
            memory
                .slice_mut(&mut caller, out_pk_ptr as usize, 65)
                .copy_from_slice(&[0u8; 65]);
            return 1;
        }
    };

    // Shared low-s policy with the Vara path (gear-core::crypto).
    if flag == 1 && !is_low_s(&sig_array) {
        memory
            .slice_mut(&mut caller, out_pk_ptr as usize, 65)
            .copy_from_slice(&[0u8; 65]);
        return 1;
    }

    // Run recovery via sp_core::ecdsa (33-byte compressed) then
    // decompress to 65 bytes with libsecp256k1. Mirrors the Vara-side
    // impl in core/processor/src/ext.rs so both networks behave
    // identically. See the note there on why we avoid sp_io::crypto
    // on this path.
    let signature = sp_core::ecdsa::Signature::from_raw(sig_array);
    let (pk_bytes, err_code) = match signature.recover_prehashed(&msg_hash) {
        Some(compressed) => {
            let compressed_slice: &[u8] = AsRef::<[u8]>::as_ref(&compressed);
            match compressed_slice
                .try_into()
                .ok()
                .and_then(|bytes: [u8; 33]| libsecp256k1::PublicKey::parse_compressed(&bytes).ok())
            {
                Some(pk) => (pk.serialize(), 0),
                None => ([0u8; 65], 1),
            }
        }
        None => ([0u8; 65], 1),
    };

    memory
        .slice_mut(&mut caller, out_pk_ptr as usize, 65)
        .copy_from_slice(&pk_bytes);

    log::trace!(target: "host_call", "secp256k1_recover(..) -> err={err_code}");

    err_code
}
