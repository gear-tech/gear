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

use crate::{Mode, VerifyReply, VerifyRequest};
use gstd::{crypto, msg};

// The signing context MUST match the one substrate / sp_core uses so that
// a signature produced off-chain with `sp_core::sr25519::Pair::sign`
// validates under both code paths. See
// https://github.com/paritytech/substrate/blob/master/primitives/core/src/sr25519.rs
const SIGNING_CTX: &[u8] = b"substrate";

#[unsafe(no_mangle)]
extern "C" fn handle() {
    let req: VerifyRequest = msg::load().expect("decode VerifyRequest");

    let ok: VerifyReply = match req.mode {
        Mode::Wasm => verify_wasm(&req) as u8,
        Mode::Syscall => verify_syscall(&req) as u8,
    };

    // Reply as raw bytes (1 byte). Using msg::reply(u8, …) goes through
    // `with_optimized_encode` which has had edge cases with scalar types;
    // reply_bytes is unambiguous.
    msg::reply_bytes([ok], 0).expect("send reply");
}

/// WASM path: interpret `schnorrkel` curve25519 ops op-by-op inside this
/// program's own WASM. Expected gas ~17B on the PolyBaskets profile —
/// this is the slow baseline we compare against.
fn verify_wasm(req: &VerifyRequest) -> bool {
    let pk = match schnorrkel::PublicKey::from_bytes(&req.pk) {
        Ok(pk) => pk,
        Err(_) => return false,
    };
    let sig = match schnorrkel::Signature::from_bytes(&req.sig) {
        Ok(sig) => sig,
        Err(_) => return false,
    };
    pk.verify_simple(SIGNING_CTX, &req.msg, &sig).is_ok()
}

/// Syscall path: one `gr_sr25519_verify` syscall. Expected gas ~150M —
/// native host compute, no in-WASM curve arithmetic.
fn verify_syscall(req: &VerifyRequest) -> bool {
    crypto::sr25519_verify(&req.pk, &req.msg, &req.sig)
}
