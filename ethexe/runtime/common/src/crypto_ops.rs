// Copyright (C) Gear Technologies Inc.
// SPDX-License-Identifier: GPL-3.0-or-later WITH Classpath-exception-2.0

//! Native `gr_crypto` operation implementations, shared by every host
//! that executes ethexe programs (ethexe-processor, gtest).
//!
//! All implementations must be strictly deterministic — they are part of
//! state computation replicated across validators.
use ark_bls12_381::{Bls12_381, G1Affine, G1Projective, G2Affine, G2Projective};
use ark_ec::{
    AffineRepr, CurveGroup, Group,
    bls12::Bls12Config,
    hashing::{HashToCurve, curve_maps::wb, map_to_curve_hasher::MapToCurveBasedHasher},
    pairing::Pairing,
};
use ark_ff::{Zero, fields::field_hashers::DefaultFieldHasher};
use ark_serialize::{CanonicalDeserialize, CanonicalSerialize};
use blake2::{Blake2b, digest::consts::U32};
use gsys::CryptoOp;
use sha2::{Digest as _, Sha256};
use sha3::Keccak256;

type Blake2b256 = Blake2b<U32>;
type WBMap = wb::WBMap<<ark_bls12_381::Config as Bls12Config>::G2Config>;

/// Ciphersuite domain separation tag — must match the one used by the
/// Vara BLS builtin (`gear_runtime_interface::DST_G2`).
const DST_G2: &[u8] = b"BLS_SIG_BLS12381G2_XMD:SHA-256_SSWU_RO_POP_";

const G1_COMPRESSED_LEN: usize = 48;
const G2_COMPRESSED_LEN: usize = 96;

pub fn execute(op: CryptoOp, input: &[u8]) -> Option<Vec<u8>> {
    match op {
        CryptoOp::Keccak256 => Some(Keccak256::digest(input).to_vec()),
        CryptoOp::Sha256 => Some(Sha256::digest(input).to_vec()),
        CryptoOp::Blake2b256 => Some(Blake2b256::digest(input).to_vec()),
        CryptoOp::Bls12381Verify => bls12_381_verify(input),
        CryptoOp::Bls12381AggregateG1 => bls12_381_aggregate_g1(input),
    }
}

/// Min-pk BLS verification: `input = pk(48) ++ signature(96) ++ message`.
/// Output is one byte: 1 — valid, 0 — invalid.
fn bls12_381_verify(input: &[u8]) -> Option<Vec<u8>> {
    const PREFIX_LEN: usize = G1_COMPRESSED_LEN + G2_COMPRESSED_LEN;
    if input.len() < PREFIX_LEN {
        return None;
    }

    let pk = G1Affine::deserialize_compressed(&input[..G1_COMPRESSED_LEN]).ok()?;
    let signature = G2Affine::deserialize_compressed(&input[G1_COMPRESSED_LEN..PREFIX_LEN]).ok()?;
    let message = &input[PREFIX_LEN..];

    // The identity public key passes the pairing check trivially, and the
    // IETF BLS spec rejects identity signatures up front — refuse both
    // (mirrors eth2 behavior).
    if pk.is_zero() || signature.is_zero() {
        return None;
    }

    let hasher =
        MapToCurveBasedHasher::<G2Projective, DefaultFieldHasher<Sha256>, WBMap>::new(DST_G2)
            .ok()?;
    let msg_point = hasher.hash(message).ok()?;

    // e(pk, H(m)) * e(-G1, sig) == 1  <=>  e(pk, H(m)) == e(G1, sig).
    // `PairingOutput` is additive, so the GT identity is `zero`.
    let valid = Bls12_381::multi_pairing(
        [pk.into_group(), -G1Projective::generator()],
        [msg_point.into_group(), signature.into_group()],
    )
    .is_zero();

    Some(vec![valid as u8])
}

/// Aggregate (sum) compressed G1 points: `input` is a non-empty
/// concatenation of 48-byte points; output is the 48-byte compressed sum.
fn bls12_381_aggregate_g1(input: &[u8]) -> Option<Vec<u8>> {
    if input.is_empty() || !input.len().is_multiple_of(G1_COMPRESSED_LEN) {
        return None;
    }

    let mut sum = G1Projective::default();
    for chunk in input.chunks_exact(G1_COMPRESSED_LEN) {
        let point = G1Affine::deserialize_compressed(chunk).ok()?;
        sum += point;
    }

    let mut output = Vec::with_capacity(G1_COMPRESSED_LEN);
    sum.into_affine().serialize_compressed(&mut output).ok()?;
    Some(output)
}

#[cfg(test)]
mod tests {
    use super::*;
    use ark_bls12_381::Fr;
    use ark_ff::UniformRand;
    use ark_std::rand::{SeedableRng, rngs::StdRng};

    mod props {
        use super::*;
        use proptest::prelude::*;

        proptest! {
            /// Host crypto must never panic on attacker-controlled
            /// input — malformed data maps to `Err`, and successful
            /// results always match the declared output size.
            #[test]
            fn execute_never_panics_and_sizes_output(
                op_raw in 0u32..8,
                input in proptest::collection::vec(any::<u8>(), 0..512),
            ) {
                let Some(op) = CryptoOp::from_u32(op_raw) else {
                    return Ok(());
                };
                if let Some(output) = execute(op, &input) {
                    prop_assert_eq!(output.len(), op.output_len() as usize);
                }
            }

            /// G1 aggregation is order-independent.
            #[test]
            fn aggregate_g1_is_order_independent(
                seeds in proptest::collection::vec(any::<u64>(), 2..5),
                swap in any::<prop::sample::Index>(),
            ) {
                let points: Vec<Vec<u8>> = seeds
                    .iter()
                    .map(|seed| {
                        let mut rng = StdRng::seed_from_u64(*seed);
                        pk_bytes(&Fr::rand(&mut rng))
                    })
                    .collect();

                let forward = execute(
                    CryptoOp::Bls12381AggregateG1,
                    &points.concat(),
                ).unwrap();

                let mut shuffled = points.clone();
                let i = swap.index(shuffled.len());
                shuffled.swap(0, i);
                let swapped = execute(
                    CryptoOp::Bls12381AggregateG1,
                    &shuffled.concat(),
                ).unwrap();

                prop_assert_eq!(forward, swapped);
            }
        }
    }

    fn hex(bytes: &[u8]) -> String {
        bytes.iter().map(|b| format!("{b:02x}")).collect()
    }

    /// NIST/keccak reference vectors for empty and "abc" inputs.
    #[test]
    fn hash_test_vectors() {
        let cases: &[(CryptoOp, &[u8], &str)] = &[
            (
                CryptoOp::Keccak256,
                b"",
                "c5d2460186f7233c927e7db2dcc703c0e500b653ca82273b7bfad8045d85a470",
            ),
            (
                CryptoOp::Keccak256,
                b"abc",
                "4e03657aea45a94fc7d47ba826c8d667c0d1e6e33a64a036ec44f58fa12d6c45",
            ),
            (
                CryptoOp::Sha256,
                b"",
                "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855",
            ),
            (
                CryptoOp::Sha256,
                b"abc",
                "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad",
            ),
            (
                CryptoOp::Blake2b256,
                b"",
                "0e5751c026e543b2e8ab2eb06099daa1d1e5df47778f7787faab45cdf12fe3a8",
            ),
            (
                CryptoOp::Blake2b256,
                b"abc",
                "bddd813c634239723171ef3fee98579b94964e3bb1cb3e427262c8c068d52319",
            ),
        ];

        for (op, input, expected) in cases {
            let digest = execute(*op, input).unwrap();
            assert_eq!(digest.len(), op.output_len() as usize);
            assert_eq!(&hex(&digest), expected, "{op:?} of {input:?}");
        }
    }

    fn sign(sk: &Fr, message: &[u8]) -> Vec<u8> {
        let hasher =
            MapToCurveBasedHasher::<G2Projective, DefaultFieldHasher<Sha256>, WBMap>::new(DST_G2)
                .unwrap();
        let sig = (hasher.hash(message).unwrap().into_group() * sk).into_affine();
        let mut out = Vec::new();
        sig.serialize_compressed(&mut out).unwrap();
        out
    }

    fn pk_bytes(sk: &Fr) -> Vec<u8> {
        let pk = (G1Projective::generator() * sk).into_affine();
        let mut out = Vec::new();
        pk.serialize_compressed(&mut out).unwrap();
        out
    }

    #[test]
    fn bls_verify_roundtrip() {
        let mut rng = StdRng::seed_from_u64(42);
        let sk = Fr::rand(&mut rng);
        let message = b"ethexe crypto syscall";

        let mut input = pk_bytes(&sk);
        input.extend(sign(&sk, message));
        input.extend_from_slice(message);

        assert_eq!(execute(CryptoOp::Bls12381Verify, &input).unwrap(), vec![1]);

        // Wrong message must not verify.
        let mut wrong = pk_bytes(&sk);
        wrong.extend(sign(&sk, message));
        wrong.extend_from_slice(b"another message");
        assert_eq!(execute(CryptoOp::Bls12381Verify, &wrong).unwrap(), vec![0]);

        // Wrong key must not verify.
        let other_sk = Fr::rand(&mut rng);
        let mut wrong_key = pk_bytes(&other_sk);
        wrong_key.extend(sign(&sk, message));
        wrong_key.extend_from_slice(message);
        assert_eq!(
            execute(CryptoOp::Bls12381Verify, &wrong_key).unwrap(),
            vec![0]
        );
    }

    #[test]
    fn bls_verify_rejects_malformed_input() {
        // Too short.
        assert!(execute(CryptoOp::Bls12381Verify, &[0u8; 100]).is_none());
        // Garbage points.
        assert!(execute(CryptoOp::Bls12381Verify, &[0xFF; 150]).is_none());
        // Identity public key is forbidden even with a "valid" layout.
        let mut input = Vec::new();
        let mut inf = Vec::new();
        G1Affine::identity().serialize_compressed(&mut inf).unwrap();
        input.extend_from_slice(&inf);
        let mut sig_inf = Vec::new();
        G2Affine::identity()
            .serialize_compressed(&mut sig_inf)
            .unwrap();
        input.extend_from_slice(&sig_inf);
        input.extend_from_slice(b"msg");
        assert!(execute(CryptoOp::Bls12381Verify, &input).is_none());

        // Identity signature under a valid public key is rejected up front.
        let mut rng = StdRng::seed_from_u64(5);
        let mut with_inf_sig = pk_bytes(&Fr::rand(&mut rng));
        let mut sig_inf = Vec::new();
        G2Affine::identity()
            .serialize_compressed(&mut sig_inf)
            .unwrap();
        with_inf_sig.extend_from_slice(&sig_inf);
        with_inf_sig.extend_from_slice(b"msg");
        assert!(execute(CryptoOp::Bls12381Verify, &with_inf_sig).is_none());
    }

    #[test]
    fn bls_aggregate_g1_matches_scalar_sum() {
        let mut rng = StdRng::seed_from_u64(7);
        let (a, b) = (Fr::rand(&mut rng), Fr::rand(&mut rng));

        let mut input = pk_bytes(&a);
        input.extend(pk_bytes(&b));
        let aggregated = execute(CryptoOp::Bls12381AggregateG1, &input).unwrap();

        assert_eq!(aggregated, pk_bytes(&(a + b)));

        // Misaligned and empty inputs are rejected.
        assert!(execute(CryptoOp::Bls12381AggregateG1, &input[..47]).is_none());
        assert!(execute(CryptoOp::Bls12381AggregateG1, &[]).is_none());
    }
}
