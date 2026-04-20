// This file is part of Gear.

// Copyright (C) 2026 Gear Technologies Inc.
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

use crate::Op;
use alloc::vec::Vec;
use gstd::{crypto, hash, msg};
use parity_scale_codec::Encode;

// The sr25519 WASM-path signing context MUST match substrate /
// sp_core so signatures signed off-chain validate under both paths.
// See https://github.com/paritytech/substrate/blob/master/primitives/core/src/sr25519.rs
const SIGNING_CTX: &[u8] = b"substrate";

// Empty init. `handle()` sees the first real payload; the gear runtime
// routes the first incoming message to `init()` by default.
#[unsafe(no_mangle)]
extern "C" fn init() {}

#[unsafe(no_mangle)]
extern "C" fn handle() {
    let op: Op = msg::load().expect("decode Op");

    let reply: Vec<u8> = match op {
        Op::Sr25519VerifyWasm { pk, msg: data, sig } => {
            alloc::vec![verify_sr25519_wasm(&pk, &data, &sig) as u8]
        }
        Op::Sr25519VerifySyscall { pk, msg: data, sig } => {
            alloc::vec![crypto::sr25519_verify(&pk, &data, &sig) as u8]
        }
        Op::Ed25519Verify { pk, msg: data, sig } => {
            alloc::vec![crypto::ed25519_verify(&pk, &data, &sig) as u8]
        }
        Op::Secp256k1Verify { msg_hash, sig, pk } => {
            alloc::vec![crypto::secp256k1_verify(&msg_hash, &sig, &pk) as u8]
        }
        Op::Secp256k1Recover { msg_hash, sig } => {
            // SCALE-encoded Option<[u8; 65]>:
            //   None       → [0x00]
            //   Some(pk65) → [0x01, pk65...]
            crypto::secp256k1_recover(&msg_hash, &sig).encode()
        }
        Op::Blake2b256(data) => hash::blake2b_256(&data).to_vec(),
        Op::Sha256(data) => hash::sha256(&data).to_vec(),
        Op::Keccak256(data) => hash::keccak256(&data).to_vec(),
    };

    msg::reply_bytes(reply, 0).expect("send reply");
}

/// WASM-path sr25519 verify: interprets curve25519 op-by-op via the
/// `schnorrkel` crate compiled into this program. Slow baseline for
/// the gas-delta comparison in `tests/gas_delta.rs`.
fn verify_sr25519_wasm(pk: &[u8; 32], msg: &[u8], sig: &[u8; 64]) -> bool {
    let pk = match schnorrkel::PublicKey::from_bytes(pk) {
        Ok(pk) => pk,
        Err(_) => return false,
    };
    let sig = match schnorrkel::Signature::from_bytes(sig) {
        Ok(sig) => sig,
        Err(_) => return false,
    };
    pk.verify_simple(SIGNING_CTX, msg, &sig).is_ok()
}
