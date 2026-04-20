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
//
// You should have received a copy of the GNU General Public License
// along with this program. If not, see <https://www.gnu.org/licenses/>.

//! Known-answer tests for each of the seven `gr_*` crypto/hash
//! syscalls. Complements `gas_delta.rs` which only exercises sr25519.
//!
//! Each test either:
//!   * uses a published reference vector (Ethereum, RFC) so a
//!     regression against the spec fails loudly, or
//!   * rolls a fresh valid input with `sp_core`, runs it through the
//!     syscall via the demo program, and asserts round-trip equality.
//!
//! Covers the full chain:
//!   guest program → gsys declaration → wasm-instrument signature →
//!   core/backend wrapper → gas charge → `Externalities` trait →
//!   Vara `Ext` impl (via gtest simulator) → reply roundtrip.

use demo_crypto::Op;
use gtest::{BlockRunResult, Program, System, constants::DEFAULT_USER_ALICE};
use parity_scale_codec::{Decode, Encode};
use sp_core::{Pair, ecdsa, ed25519};

// ============================================================
// Hash syscalls — hardcoded Ethereum/NIST test vectors.
// ============================================================

/// BLAKE2b-256 round-trip: compare on-chain digest against `sp_core`'s
/// native `blake2_256` for several inputs of varying length. Covers
/// the base cost + per-byte path at 0 / 32 / 256 / 1024 bytes.
#[test]
fn blake2b_256_roundtrip() {
    let sys = System::new();
    sys.init_logger();
    let (prog, from) = setup(&sys);

    for len in [0usize, 32, 256, 1024] {
        let data: Vec<u8> = (0..len).map(|i| (i & 0xff) as u8).collect();
        let expected = sp_core::hashing::blake2_256(&data);

        let reply = send_op(&sys, &prog, from, Op::Blake2b256(data));
        assert_eq!(
            reply.as_slice(),
            expected.as_slice(),
            "blake2b_256 mismatch at len={len}"
        );
    }
}

/// SHA-256 KAT: `sha256("abc")` from FIPS 180-4 Appendix B.1.
/// Also round-trips larger inputs against `sp_core::hashing::sha2_256`.
#[test]
fn sha256_kat_and_roundtrip() {
    let sys = System::new();
    sys.init_logger();
    let (prog, from) = setup(&sys);

    // FIPS 180-4 Appendix B.1: SHA-256("abc")
    //   = BA7816BF 8F01CFEA 414140DE 5DAE2223 B00361A3 96177A9C B410FF61 F20015AD
    let kat_input = b"abc".to_vec();
    let kat_expected: [u8; 32] = [
        0xba, 0x78, 0x16, 0xbf, 0x8f, 0x01, 0xcf, 0xea, 0x41, 0x41, 0x40, 0xde, 0x5d, 0xae, 0x22,
        0x23, 0xb0, 0x03, 0x61, 0xa3, 0x96, 0x17, 0x7a, 0x9c, 0xb4, 0x10, 0xff, 0x61, 0xf2, 0x00,
        0x15, 0xad,
    ];
    let reply = send_op(&sys, &prog, from, Op::Sha256(kat_input));
    assert_eq!(
        reply.as_slice(),
        kat_expected.as_slice(),
        "SHA-256(\"abc\") KAT mismatch (FIPS 180-4 B.1)"
    );

    for len in [0usize, 64, 1024] {
        let data: Vec<u8> = (0..len).map(|i| (i & 0xff) as u8).collect();
        let expected = sp_core::hashing::sha2_256(&data);
        let reply = send_op(&sys, &prog, from, Op::Sha256(data));
        assert_eq!(reply.as_slice(), expected.as_slice(), "sha256 len={len}");
    }
}

/// Keccak-256 KAT: Ethereum-style Keccak of the empty string.
///   keccak256("") = c5d2460186f7233c927e7db2dcc703c0e500b653ca82273b7bfad8045d85a470
/// This is the most common sanity check for "did we wire Keccak (not
/// SHA-3) correctly" — a SHA-3-256("") would produce a different output.
#[test]
fn keccak256_kat_and_roundtrip() {
    let sys = System::new();
    sys.init_logger();
    let (prog, from) = setup(&sys);

    // keccak256("")
    let kat_expected: [u8; 32] = [
        0xc5, 0xd2, 0x46, 0x01, 0x86, 0xf7, 0x23, 0x3c, 0x92, 0x7e, 0x7d, 0xb2, 0xdc, 0xc7, 0x03,
        0xc0, 0xe5, 0x00, 0xb6, 0x53, 0xca, 0x82, 0x27, 0x3b, 0x7b, 0xfa, 0xd8, 0x04, 0x5d, 0x85,
        0xa4, 0x70,
    ];
    let reply = send_op(&sys, &prog, from, Op::Keccak256(Vec::new()));
    assert_eq!(
        reply.as_slice(),
        kat_expected.as_slice(),
        "keccak256(\"\") KAT mismatch (guards against accidental SHA-3)"
    );

    for len in [32usize, 256] {
        let data: Vec<u8> = (0..len).map(|i| (i & 0xff) as u8).collect();
        let expected = sp_core::hashing::keccak_256(&data);
        let reply = send_op(&sys, &prog, from, Op::Keccak256(data));
        assert_eq!(reply.as_slice(), expected.as_slice(), "keccak256 len={len}");
    }
}

// ============================================================
// Verify syscalls — positive + negative per curve.
// ============================================================

#[test]
fn ed25519_verify_valid_and_tampered() {
    let sys = System::new();
    sys.init_logger();
    let (prog, from) = setup(&sys);

    let (pair, _) = ed25519::Pair::generate();
    let pk: [u8; 32] = pair.public().0;
    let msg: Vec<u8> = b"ed25519-kat".to_vec();
    let sig: [u8; 64] = pair.sign(&msg).0;

    let reply = send_op(
        &sys,
        &prog,
        from,
        Op::Ed25519Verify {
            pk,
            msg: msg.clone(),
            sig,
        },
    );
    assert_eq!(reply, vec![1u8], "ed25519 valid triple must verify");

    // Tamper with one bit of the signature — must reject.
    let mut bad_sig = sig;
    bad_sig[0] ^= 0x01;
    let reply = send_op(
        &sys,
        &prog,
        from,
        Op::Ed25519Verify {
            pk,
            msg,
            sig: bad_sig,
        },
    );
    assert_eq!(reply, vec![0u8], "tampered ed25519 sig must fail verify");
}

#[test]
fn secp256k1_verify_valid_and_tampered() {
    let sys = System::new();
    sys.init_logger();
    let (prog, from) = setup(&sys);

    let (pair, _) = ecdsa::Pair::generate();
    let pk: [u8; 33] = pair.public().0;
    let msg_hash: [u8; 32] = sp_core::hashing::blake2_256(b"secp256k1-kat");
    let sig: [u8; 65] = pair.sign_prehashed(&msg_hash).0;

    let reply = send_op(
        &sys,
        &prog,
        from,
        Op::Secp256k1Verify {
            msg_hash,
            sig,
            pk,
        },
    );
    assert_eq!(reply, vec![1u8], "secp256k1 valid triple must verify");

    // Tamper with one byte of r — must reject.
    let mut bad_sig = sig;
    bad_sig[0] ^= 0x01;
    let reply = send_op(
        &sys,
        &prog,
        from,
        Op::Secp256k1Verify {
            msg_hash,
            sig: bad_sig,
            pk,
        },
    );
    assert_eq!(reply, vec![0u8], "tampered secp256k1 sig must fail verify");

    // Tamper with msg_hash — must reject (the sig was for a different hash).
    let mut bad_hash = msg_hash;
    bad_hash[0] ^= 0x01;
    let reply = send_op(
        &sys,
        &prog,
        from,
        Op::Secp256k1Verify {
            msg_hash: bad_hash,
            sig,
            pk,
        },
    );
    assert_eq!(
        reply,
        vec![0u8],
        "secp256k1 verify of sig against wrong hash must fail"
    );
}

// ============================================================
// secp256k1 recover — full ecrecover pipeline.
// ============================================================

#[test]
fn secp256k1_recover_matches_signer_and_rejects_malformed() {
    let sys = System::new();
    sys.init_logger();
    let (prog, from) = setup(&sys);

    let (pair, _) = ecdsa::Pair::generate();
    // sp_core's compressed pk → libsecp256k1 decompression to produce
    // the 65-byte SEC1 uncompressed form we expect back.
    let compressed: [u8; 33] = pair.public().0;
    let expected_uncompressed: [u8; 65] =
        libsecp256k1::PublicKey::parse_compressed(&compressed)
            .expect("decompress signer pk")
            .serialize();

    let msg_hash: [u8; 32] = sp_core::hashing::blake2_256(b"secp256k1-recover-kat");
    let sig: [u8; 65] = pair.sign_prehashed(&msg_hash).0;

    // Success path.
    let reply = send_op(&sys, &prog, from, Op::Secp256k1Recover { msg_hash, sig });
    let recovered: Option<[u8; 65]> =
        Option::<[u8; 65]>::decode(&mut &reply[..]).expect("decode Option<[u8;65]>");
    let recovered = recovered.expect("recover on valid sig must return Some");
    assert_eq!(recovered[0], 0x04, "recovered pk must use SEC1 0x04 tag");
    assert_eq!(
        recovered, expected_uncompressed,
        "recovered pk must match signer"
    );

    // Malformed sig (all zeros): recovery must return None without trap.
    let bad_sig = [0u8; 65];
    let reply = send_op(
        &sys,
        &prog,
        from,
        Op::Secp256k1Recover {
            msg_hash,
            sig: bad_sig,
        },
    );
    let recovered: Option<[u8; 65]> =
        Option::<[u8; 65]>::decode(&mut &reply[..]).expect("decode Option<[u8;65]>");
    assert!(
        recovered.is_none(),
        "all-zero sig must fail recovery (got {recovered:?})"
    );
}

// ============================================================
// Helpers
// ============================================================

fn setup(system: &System) -> (Program<'_>, u64) {
    let prog = Program::current(system);
    let from = DEFAULT_USER_ALICE;
    // init() is empty but must still be dispatched before the first
    // handle() call — gear routes the first message to init on a
    // fresh program.
    let init_id = prog.send_bytes(from, []);
    let run = system.run_next_block();
    assert!(
        run.succeed.contains(&init_id),
        "program init must succeed before KAT runs"
    );
    (prog, from)
}

fn send_op(system: &System, prog: &Program, from: u64, op: Op) -> Vec<u8> {
    let msg_id = prog.send_bytes(from, op.encode());
    let run: BlockRunResult = system.run_next_block();
    assert!(
        run.succeed.contains(&msg_id),
        "op failed to succeed (failed={}, not_executed={})",
        run.failed.contains(&msg_id),
        run.not_executed.contains(&msg_id),
    );
    run.log
        .iter()
        .find(|e| e.destination() == from.into() && !e.payload().is_empty())
        .expect("program replied with a non-empty payload")
        .payload()
        .to_vec()
}
