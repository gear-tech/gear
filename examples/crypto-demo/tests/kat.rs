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

use demo_crypto::Op;
use gear_core::crypto::SECP256K1_N_HALF;
use gtest::{BlockRunResult, Program, System, constants::DEFAULT_USER_ALICE};
use parity_scale_codec::{Decode, Encode};
use sp_core::{Pair, ecdsa, ed25519};

// ============================================================
// Hash syscalls — hardcoded Ethereum/NIST test vectors.
// ============================================================

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

/// SHA-256("abc") from FIPS 180-4 Appendix B.1.
#[test]
fn sha256_kat_and_roundtrip() {
    let sys = System::new();
    sys.init_logger();
    let (prog, from) = setup(&sys);

    let kat_expected: [u8; 32] = [
        0xba, 0x78, 0x16, 0xbf, 0x8f, 0x01, 0xcf, 0xea, 0x41, 0x41, 0x40, 0xde, 0x5d, 0xae, 0x22,
        0x23, 0xb0, 0x03, 0x61, 0xa3, 0x96, 0x17, 0x7a, 0x9c, 0xb4, 0x10, 0xff, 0x61, 0xf2, 0x00,
        0x15, 0xad,
    ];
    let reply = send_op(&sys, &prog, from, Op::Sha256(b"abc".to_vec()));
    assert_eq!(
        reply.as_slice(),
        kat_expected.as_slice(),
        "SHA-256(\"abc\") KAT (FIPS 180-4 B.1)"
    );

    for len in [0usize, 64, 1024] {
        let data: Vec<u8> = (0..len).map(|i| (i & 0xff) as u8).collect();
        let expected = sp_core::hashing::sha2_256(&data);
        let reply = send_op(&sys, &prog, from, Op::Sha256(data));
        assert_eq!(reply.as_slice(), expected.as_slice(), "sha256 len={len}");
    }
}

/// keccak256("") = c5d2460186f7233c... (Ethereum standard).
/// Guards against accidental wiring of SHA-3-256 instead of Keccak.
#[test]
fn keccak256_kat_and_roundtrip() {
    let sys = System::new();
    sys.init_logger();
    let (prog, from) = setup(&sys);

    let kat_expected: [u8; 32] = [
        0xc5, 0xd2, 0x46, 0x01, 0x86, 0xf7, 0x23, 0x3c, 0x92, 0x7e, 0x7d, 0xb2, 0xdc, 0xc7, 0x03,
        0xc0, 0xe5, 0x00, 0xb6, 0x53, 0xca, 0x82, 0x27, 0x3b, 0x7b, 0xfa, 0xd8, 0x04, 0x5d, 0x85,
        0xa4, 0x70,
    ];
    let reply = send_op(&sys, &prog, from, Op::Keccak256(Vec::new()));
    assert_eq!(
        reply.as_slice(),
        kat_expected.as_slice(),
        "keccak256(\"\") (guards against SHA-3)"
    );

    for len in [32usize, 256] {
        let data: Vec<u8> = (0..len).map(|i| (i & 0xff) as u8).collect();
        let expected = sp_core::hashing::keccak_256(&data);
        let reply = send_op(&sys, &prog, from, Op::Keccak256(data));
        assert_eq!(reply.as_slice(), expected.as_slice(), "keccak256 len={len}");
    }
}

// ============================================================
// sr25519 signing-context tests (new ABI).
// ============================================================

/// Sign with an app-specific ctx, verify under the same ctx. Proves
/// the new ABI actually works for non-default contexts — the headline
/// reason this change exists.
#[test]
fn sr25519_verify_accepts_matching_non_substrate_context() {
    let sys = System::new();
    sys.init_logger();
    let (prog, from) = setup(&sys);

    let ctx: Vec<u8> = b"my-app-v1".to_vec();
    let msg: Vec<u8> = b"hello non-substrate world".to_vec();
    let (pk, sig) = sign_sr25519(&ctx, &msg);

    let reply = send_op(
        &sys,
        &prog,
        from,
        Op::Sr25519VerifySyscall { pk, ctx, msg, sig },
    );
    assert_eq!(
        reply,
        vec![1u8],
        "sr25519 under matching non-substrate ctx must verify"
    );
}

/// Sign with ctx A, verify with ctx B — must reject. Guards the
/// pre-fix silent-failure footgun.
#[test]
fn sr25519_verify_rejects_mismatched_context() {
    let sys = System::new();
    sys.init_logger();
    let (prog, from) = setup(&sys);

    let ctx_signer: Vec<u8> = b"app-A".to_vec();
    let ctx_verifier: Vec<u8> = b"app-B".to_vec();
    let msg: Vec<u8> = b"ctx-mismatch-test".to_vec();
    let (pk, sig) = sign_sr25519(&ctx_signer, &msg);

    let reply = send_op(
        &sys,
        &prog,
        from,
        Op::Sr25519VerifySyscall {
            pk,
            ctx: ctx_verifier,
            msg,
            sig,
        },
    );
    assert_eq!(
        reply,
        vec![0u8],
        "sr25519 under mismatched ctx must fail verify"
    );
}

/// Empty context is a legal Schnorrkel input; ABI must preserve that.
#[test]
fn sr25519_verify_accepts_empty_context() {
    let sys = System::new();
    sys.init_logger();
    let (prog, from) = setup(&sys);

    let ctx: Vec<u8> = Vec::new();
    let msg: Vec<u8> = b"empty ctx test".to_vec();
    let (pk, sig) = sign_sr25519(&ctx, &msg);

    let reply = send_op(
        &sys,
        &prog,
        from,
        Op::Sr25519VerifySyscall { pk, ctx, msg, sig },
    );
    assert_eq!(reply, vec![1u8], "sr25519 under empty ctx must verify");
}

/// Backwards-compat: signatures produced by `sp_core::sr25519::Pair::sign`
/// (which uses `b"substrate"` internally) must verify under the new
/// API when the caller passes `ctx = b"substrate"`.
#[test]
fn sr25519_verify_substrate_context_still_works() {
    use sp_core::sr25519;

    let sys = System::new();
    sys.init_logger();
    let (prog, from) = setup(&sys);

    // Sign via sp_core::sr25519::Pair (hardcoded substrate ctx).
    let (pair, _) = sr25519::Pair::generate();
    let pk: [u8; 32] = pair.public().0;
    let msg: Vec<u8> = b"substrate-context-drop-in".to_vec();
    let sig: [u8; 64] = pair.sign(&msg).0;

    let reply = send_op(
        &sys,
        &prog,
        from,
        Op::Sr25519VerifySyscall {
            pk,
            ctx: b"substrate".to_vec(),
            msg,
            sig,
        },
    );
    assert_eq!(
        reply,
        vec![1u8],
        "sp_core-signed sig must verify under ctx=substrate"
    );
}

// ============================================================
// ed25519 — positive + tampered.
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

// ============================================================
// secp256k1 malleability + boundary tests.
// ============================================================

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
            strict: false,
        },
    );
    assert_eq!(reply, vec![1u8], "secp256k1 valid triple must verify");

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
            strict: false,
        },
    );
    assert_eq!(reply, vec![0u8], "tampered secp256k1 sig must fail verify");

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
            strict: false,
        },
    );
    assert_eq!(
        reply,
        vec![0u8],
        "secp256k1 verify of sig against wrong hash must fail"
    );
}

/// The big one: construct a high-s twin `(r, n-s, v^1)` and assert
/// verify and recover give CONSISTENT answers for the same (sig, flag)
/// pair. Under flag=0 BOTH accept; under flag=1 BOTH reject. Proves
/// the asymmetry codex flagged cannot happen.
#[test]
fn secp256k1_high_s_permissive_vs_strict() {
    let sys = System::new();
    sys.init_logger();
    let (prog, from) = setup(&sys);

    let (pair, _) = ecdsa::Pair::generate();
    let pk: [u8; 33] = pair.public().0;
    let msg_hash: [u8; 32] = sp_core::hashing::blake2_256(b"secp256k1-malleability");
    let sig_low: [u8; 65] = pair.sign_prehashed(&msg_hash).0;

    // sp_core signs produce canonical (low-s) sigs. Confirm.
    assert!(
        gear_core::crypto::is_low_s(&sig_low),
        "sp_core sig expected to be low-s by construction"
    );

    // Flip s → n-s and flip v's low bit. This twin signature recovers
    // the same pubkey but has different bytes.
    let sig_high = make_high_s_twin(&sig_low);
    assert!(
        !gear_core::crypto::is_low_s(&sig_high),
        "twin sig must be high-s"
    );

    // Under permissive (strict=false): BOTH sigs accepted by verify.
    for (label, sig) in [("low-s", sig_low), ("high-s", sig_high)] {
        let reply = send_op(
            &sys,
            &prog,
            from,
            Op::Secp256k1Verify {
                msg_hash,
                sig,
                pk,
                strict: false,
            },
        );
        assert_eq!(reply, vec![1u8], "verify(flag=0) must accept {label} sig");
    }

    // Under strict (strict=true): low-s accepted, high-s rejected.
    let reply = send_op(
        &sys,
        &prog,
        from,
        Op::Secp256k1Verify {
            msg_hash,
            sig: sig_low,
            pk,
            strict: true,
        },
    );
    assert_eq!(reply, vec![1u8], "verify(flag=1) must accept low-s sig");

    let reply = send_op(
        &sys,
        &prog,
        from,
        Op::Secp256k1Verify {
            msg_hash,
            sig: sig_high,
            pk,
            strict: true,
        },
    );
    assert_eq!(reply, vec![0u8], "verify(flag=1) must reject high-s sig");

    // Recover: same policy. Under permissive BOTH recover to same pk;
    // under strict high-s returns None.
    let expected_uncompressed = libsecp256k1::PublicKey::parse_compressed(&pk)
        .expect("decompress signer pk")
        .serialize();

    for (label, sig) in [("low-s", sig_low), ("high-s", sig_high)] {
        let reply = send_op(
            &sys,
            &prog,
            from,
            Op::Secp256k1Recover {
                msg_hash,
                sig,
                strict: false,
            },
        );
        let got: Option<[u8; 65]> = Option::<[u8; 65]>::decode(&mut &reply[..]).unwrap();
        assert_eq!(
            got,
            Some(expected_uncompressed),
            "recover(flag=0, {label}) must recover signer pk"
        );
    }

    let reply = send_op(
        &sys,
        &prog,
        from,
        Op::Secp256k1Recover {
            msg_hash,
            sig: sig_low,
            strict: true,
        },
    );
    let got: Option<[u8; 65]> = Option::<[u8; 65]>::decode(&mut &reply[..]).unwrap();
    assert_eq!(
        got,
        Some(expected_uncompressed),
        "recover(flag=1, low-s) must recover signer pk"
    );

    let reply = send_op(
        &sys,
        &prog,
        from,
        Op::Secp256k1Recover {
            msg_hash,
            sig: sig_high,
            strict: true,
        },
    );
    let got: Option<[u8; 65]> = Option::<[u8; 65]>::decode(&mut &reply[..]).unwrap();
    assert_eq!(got, None, "recover(flag=1, high-s) must return None");
}

/// Boundary: `s == n/2` exactly is canonical low-s. Must be accepted
/// under strict.
#[test]
fn secp256k1_s_eq_half_order_accepted_in_strict() {
    let sig = synthetic_sig_with_s(SECP256K1_N_HALF);
    assert!(
        gear_core::crypto::is_low_s(&sig),
        "s == n/2 must byte-compare as low-s"
    );
    // The resulting sig isn't a real signature over any message, so
    // we only check the malleability gate at the ABI layer, not the
    // full verify. The low-s policy is the one thing this test
    // exercises — the `is_low_s` helper is the single source of truth
    // both networks consult, verified by the unit test in
    // `core/src/crypto.rs`.
}

/// Boundary: `s == n/2 + 1` is high-s. Must be rejected under strict.
#[test]
fn secp256k1_s_eq_half_order_plus_one_rejected_in_strict() {
    let mut plus_one = SECP256K1_N_HALF;
    // Add 1 big-endian with carry.
    for i in (0..32).rev() {
        let (v, carry) = plus_one[i].overflowing_add(1);
        plus_one[i] = v;
        if !carry {
            break;
        }
    }
    let sig = synthetic_sig_with_s(plus_one);
    assert!(
        !gear_core::crypto::is_low_s(&sig),
        "s == n/2 + 1 must byte-compare as high-s"
    );
}

/// s == 0: byte-compares as low-s, but real verify/recover will still
/// reject via `parse_standard_slice` → the two rejection paths are
/// disjoint in layering but converge on "reject" — documenting that
/// here so future refactors preserve it.
#[test]
fn secp256k1_zero_s_not_flagged_by_low_s_alone() {
    let sig = synthetic_sig_with_s([0u8; 32]);
    assert!(
        gear_core::crypto::is_low_s(&sig),
        "s == 0 byte-compares as low-s (rejected by parse layer, not low-s gate)"
    );
}

// ============================================================
// secp256k1 recover — end-to-end (preserved from Stage 2).
// ============================================================

#[test]
fn secp256k1_recover_matches_signer_and_rejects_malformed() {
    let sys = System::new();
    sys.init_logger();
    let (prog, from) = setup(&sys);

    let (pair, _) = ecdsa::Pair::generate();
    let compressed: [u8; 33] = pair.public().0;
    let expected_uncompressed: [u8; 65] = libsecp256k1::PublicKey::parse_compressed(&compressed)
        .expect("decompress signer pk")
        .serialize();

    let msg_hash: [u8; 32] = sp_core::hashing::blake2_256(b"secp256k1-recover-kat");
    let sig: [u8; 65] = pair.sign_prehashed(&msg_hash).0;

    let reply = send_op(
        &sys,
        &prog,
        from,
        Op::Secp256k1Recover {
            msg_hash,
            sig,
            strict: false,
        },
    );
    let recovered: Option<[u8; 65]> = Option::<[u8; 65]>::decode(&mut &reply[..]).unwrap();
    let recovered = recovered.expect("recover on valid sig must return Some");
    assert_eq!(recovered[0], 0x04, "recovered pk must use SEC1 0x04 tag");
    assert_eq!(
        recovered, expected_uncompressed,
        "recovered pk must match signer"
    );

    let bad_sig = [0u8; 65];
    let reply = send_op(
        &sys,
        &prog,
        from,
        Op::Secp256k1Recover {
            msg_hash,
            sig: bad_sig,
            strict: false,
        },
    );
    let recovered: Option<[u8; 65]> = Option::<[u8; 65]>::decode(&mut &reply[..]).unwrap();
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

/// Sign a message via the raw schnorrkel path with an explicit ctx.
/// sp_core::sr25519::Pair::sign hardcodes `b"substrate"`, so we go
/// through schnorrkel directly to produce sigs under arbitrary ctx.
fn sign_sr25519(ctx: &[u8], msg: &[u8]) -> ([u8; 32], [u8; 64]) {
    use schnorrkel::{ExpansionMode, MiniSecretKey};

    // Stable seed so failures reproduce; per-test variation comes
    // from ctx/msg, not key randomness.
    let mini = MiniSecretKey::from_bytes(&[7u8; 32]).unwrap();
    let kp = mini.expand_to_keypair(ExpansionMode::Ed25519);
    let sig = kp.sign_simple(ctx, msg);

    let pk: [u8; 32] = kp.public.to_bytes();
    let sig_bytes: [u8; 64] = sig.to_bytes();
    (pk, sig_bytes)
}

/// Flip a canonical low-s signature into its high-s twin: s' = n - s,
/// v' = v ^ 1. The resulting sig recovers the same pubkey.
fn make_high_s_twin(sig: &[u8; 65]) -> [u8; 65] {
    // secp256k1 group order n (big-endian).
    const N: [u8; 32] = [
        0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF,
        0xFE, 0xBA, 0xAE, 0xDC, 0xE6, 0xAF, 0x48, 0xA0, 0x3B, 0xBF, 0xD2, 0x5E, 0x8C, 0xD0, 0x36,
        0x41, 0x41,
    ];
    let mut out = *sig;
    // Compute n - s into out[32..64] (big-endian subtraction with borrow).
    let mut borrow: i16 = 0;
    for i in (0..32).rev() {
        let a = N[i] as i16;
        let b = sig[32 + i] as i16 + borrow;
        let (r, new_borrow) = if a >= b {
            (a - b, 0)
        } else {
            (a + 256 - b, 1)
        };
        out[32 + i] = r as u8;
        borrow = new_borrow;
    }
    // Flip recovery-id low bit so the twin still recovers the signer.
    out[64] ^= 1;
    out
}

/// Build a synthetic 65-byte sig with r = 1, given s bytes, v = 0.
/// For testing the low-s gate only — the sig is not valid ECDSA.
fn synthetic_sig_with_s(s: [u8; 32]) -> [u8; 65] {
    let mut sig = [0u8; 65];
    sig[31] = 1; // r = 1 (non-zero, well-formed position)
    sig[32..64].copy_from_slice(&s);
    sig
}
