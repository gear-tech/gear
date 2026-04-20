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

//! Native signature-verification primitives exposed as `gr_*` syscalls.
//!
//! Performing a signature check via these wrappers costs ~150M gas,
//! versus ~17B gas for the equivalent pure-WASM implementation.

/// Verify an sr25519 (schnorrkel/Ristretto25519) signature.
///
/// Returns `true` when `sig` is a valid signature of `msg` under `pk`,
/// `false` otherwise. Malformed keys or signatures return `false` without
/// trapping.
///
/// Dispatches to `gsys::gr_sr25519_verify`. On Vara the work runs as
/// native `sp_core::sr25519::Pair::verify`; on ethexe the same native
/// implementation runs on the host side of a wasmtime
/// `ext_sr25519_verify_v1` import.
///
/// # Examples
///
/// ```rust,ignore
/// let ok = gcore::crypto::sr25519_verify(&pk, b"hello", &sig);
/// assert!(ok);
/// ```
/// Verify an sr25519 signature using an explicit Schnorrkel simple
/// signing context.
///
/// Both signer and verifier must use the same `ctx` bytes. Passing
/// `ctx = b"substrate"` matches `sp_core::sr25519::Pair::sign`'s
/// default. See also [`sr25519_verify_substrate`] for callers that
/// want that default without typing the string.
pub fn sr25519_verify(pk: &[u8; 32], ctx: &[u8], msg: &[u8], sig: &[u8; 64]) -> bool {
    let mut ok: u8 = 0;
    unsafe {
        gsys::gr_sr25519_verify(
            pk.as_ptr() as _,
            ctx.as_ptr() as _,
            ctx.len() as u32,
            msg.as_ptr() as _,
            msg.len() as u32,
            sig.as_ptr() as _,
            &mut ok,
        );
    }
    ok != 0
}

/// Convenience wrapper around [`sr25519_verify`] that uses the
/// `b"substrate"` signing context — the default for
/// `sp_core::sr25519::Pair::sign` / `verify`. Use this for verifying
/// signatures produced by any Substrate-stack signer that doesn't
/// override the context.
pub fn sr25519_verify_substrate(pk: &[u8; 32], msg: &[u8], sig: &[u8; 64]) -> bool {
    sr25519_verify(pk, b"substrate", msg, sig)
}

/// Verify an ed25519 signature.
///
/// Same shape and error convention as [`sr25519_verify`]; the only
/// difference is the curve used server-side.
pub fn ed25519_verify(pk: &[u8; 32], msg: &[u8], sig: &[u8; 64]) -> bool {
    let mut ok: u8 = 0;
    unsafe {
        gsys::gr_ed25519_verify(
            pk.as_ptr() as _,
            msg.as_ptr() as _,
            msg.len() as u32,
            sig.as_ptr() as _,
            &mut ok,
        );
    }
    ok != 0
}

/// Verify a secp256k1 ECDSA signature over `msg_hash` against the
/// SEC1-compressed (33-byte) public key `pk`.
///
/// `msg_hash` must already be hashed (the syscall verifies on the raw
/// digest). `sig` is the 65-byte `r || s || v` form used by Ethereum
/// ecrecover; the `v` byte is ignored for verify.
/// Verify a secp256k1 ECDSA signature under the permissive malleability
/// policy (any valid sig accepted — Ethereum `ecrecover` compat).
///
/// For strict-mode verification (rejects high-s sigs at the ABI), see
/// [`secp256k1_verify_strict`].
pub fn secp256k1_verify(msg_hash: &[u8; 32], sig: &[u8; 65], pk: &[u8; 33]) -> bool {
    secp256k1_verify_with_flag(msg_hash, sig, pk, 0)
}

/// Verify a secp256k1 ECDSA signature, rejecting high-s signatures.
///
/// Use this for replay-protection paths where signature bytes are
/// hashed as a nonce — accepts only the canonical low-s form, so
/// `(r, n-s, v^1)` can't sneak through as a distinct "new" signature.
pub fn secp256k1_verify_strict(msg_hash: &[u8; 32], sig: &[u8; 65], pk: &[u8; 33]) -> bool {
    secp256k1_verify_with_flag(msg_hash, sig, pk, 1)
}

fn secp256k1_verify_with_flag(
    msg_hash: &[u8; 32],
    sig: &[u8; 65],
    pk: &[u8; 33],
    malleability_flag: u32,
) -> bool {
    let mut ok: u8 = 0;
    unsafe {
        gsys::gr_secp256k1_verify(
            msg_hash.as_ptr() as _,
            sig.as_ptr() as _,
            pk.as_ptr() as _,
            malleability_flag,
            &mut ok,
        );
    }
    ok != 0
}

/// Recover a secp256k1 public key from a signature.
///
/// Returns `Some(pk)` with the 65-byte SEC1-uncompressed pubkey
/// (`0x04 || x || y`) on success, `None` on any failure (malformed
/// signature or non-recoverable). Mirrors Ethereum's `ecrecover`
/// precompile.
///
/// # ECDSA signature malleability
///
/// ECDSA signatures are malleable: if `(r, s, v)` recovers a public
/// key, then `(r, n-s, v ^ 1)` recovers the same key. This function
/// does NOT canonicalize `s` to the low-half value (`s <= n/2`).
/// Callers that use signature bytes for replay-protection nonces,
/// deduplication, or on-chain uniqueness MUST enforce low-s before
/// accepting the signature — otherwise an attacker can flip
/// `s` → `n-s` to produce a distinct-but-equivalent signature.
pub fn secp256k1_recover(msg_hash: &[u8; 32], sig: &[u8; 65]) -> Option<[u8; 65]> {
    secp256k1_recover_with_flag(msg_hash, sig, 0)
}

/// Recover a secp256k1 pubkey, rejecting high-s signatures at the ABI.
///
/// Same API as [`secp256k1_recover`] but applies the strict
/// malleability policy. See the note on malleability on
/// [`secp256k1_recover`] for why this matters — this helper lets
/// callers opt into the guard without hand-rolling a low-s check.
pub fn secp256k1_recover_strict(msg_hash: &[u8; 32], sig: &[u8; 65]) -> Option<[u8; 65]> {
    secp256k1_recover_with_flag(msg_hash, sig, 1)
}

fn secp256k1_recover_with_flag(
    msg_hash: &[u8; 32],
    sig: &[u8; 65],
    malleability_flag: u32,
) -> Option<[u8; 65]> {
    let mut out_pk = [0u8; 65];
    let mut err: u32 = 0;
    unsafe {
        gsys::gr_secp256k1_recover(
            msg_hash.as_ptr() as _,
            sig.as_ptr() as _,
            malleability_flag,
            out_pk.as_mut_ptr() as _,
            &mut err,
        );
    }
    if err == 0 { Some(out_pk) } else { None }
}
