//! Known-answer tests against the independent Python (py_ecc) reference.
//!
//! What this test cross-checks bit-for-bit:
//!   1. derive_id(): both impls produce identical 32-byte ids for the same inputs.
//!   2. hash_to_G1(): both impls produce the same compressed G1 point from id.
//!   3. Master pub key: S·g₂ matches between impls.
//!   4. Share pub keys: Sᵢ·g₂ matches.
//!   5. Decryption shares: Dᵢ = Sᵢ · Q_id matches.
//!   6. Pairing soundness: e(D, U) and e(Q_id, AggPub)^u produce a working
//!      Rust-side encrypt-then-decrypt roundtrip.
//!
//! What we do NOT cross-check: the AEAD ciphertext body itself, because the
//! HKDF input depends on GT (Fq12) serialization, and the coefficient ordering
//! in the towering Fq6/Fq12 construction is not guaranteed to match between
//! py_ecc and arkworks. The roundtrip in step 6 still ensures end-to-end
//! correctness on the Rust side.

use ark_bls12_381::{Fr, G1Affine, G2Affine};
use ark_ec::{AffineRepr, CurveGroup};
use ark_ff::PrimeField;
use ark_serialize::CanonicalSerialize;
use ark_std::rand::{SeedableRng, rngs::StdRng};
use ethexe_tpke::*;
use serde::Deserialize;

#[derive(Debug, Deserialize)]
struct Vectors {
    #[allow(dead_code)]
    version: String,
    dst: String,
    vectors: Vec<Vector>,
}

#[derive(Debug, Deserialize)]
struct Vector {
    label: String,
    chain_id: u64,
    key_epoch_id: u32,
    threshold: u32,
    n: u32,
    plaintext_hex: String,
    user_nonce_hex: String,
    #[allow(dead_code)]
    u_scalar_hex: String,
    #[allow(dead_code)]
    master_secret_hex: String,
    poly_coeffs_hex: Vec<String>,
    id_hex: String,
    master_pub_compressed_hex: String,
    share_pubs_compressed_hex: Vec<IndexedHex>,
    secret_shares_hex: Vec<IndexedScalarHex>,
    #[allow(dead_code)]
    envelope_u_hex: String,
    #[allow(dead_code)]
    envelope_body_hex: String,
    expected_decryption_shares_hex: Vec<IndexedHex>,
}

#[derive(Debug, Deserialize)]
struct IndexedHex {
    index: u32,
    bytes_hex: String,
}

#[derive(Debug, Deserialize)]
struct IndexedScalarHex {
    index: u32,
    scalar_hex: String,
}

const VECTORS_JSON: &str = include_str!("kat/vectors.json");

fn fr_from_be_hex(h: &str) -> Fr {
    let bytes = hex::decode(h).expect("hex");
    assert_eq!(bytes.len(), 32, "expected 32-byte big-endian scalar");
    Fr::from_be_bytes_mod_order(&bytes)
}

fn g1_compressed_hex(p: &G1Affine) -> String {
    let mut buf = [0u8; 48];
    p.serialize_compressed(&mut buf[..]).unwrap();
    hex::encode(buf)
}

fn g2_compressed_hex(p: &G2Affine) -> String {
    let mut buf = [0u8; 96];
    p.serialize_compressed(&mut buf[..]).unwrap();
    hex::encode(buf)
}

#[test]
fn dst_matches_reference() {
    let v: Vectors = serde_json::from_str(VECTORS_JSON).expect("parse vectors.json");
    assert_eq!(v.dst.as_bytes(), DST_G1, "DST string mismatch");
}

#[test]
fn cross_check_every_vector() {
    let v: Vectors = serde_json::from_str(VECTORS_JSON).expect("parse vectors.json");
    for vec in &v.vectors {
        cross_check_one(vec);
    }
}

fn cross_check_one(v: &Vector) {
    eprintln!("KAT: {}", v.label);

    let plaintext = hex::decode(&v.plaintext_hex).unwrap();
    let user_nonce_vec = hex::decode(&v.user_nonce_hex).unwrap();
    let user_nonce: [u8; 32] = user_nonce_vec.as_slice().try_into().unwrap();

    // (1) derive_id: must match Python output.
    let id = derive_id(v.chain_id, v.key_epoch_id, &plaintext, &user_nonce);
    assert_eq!(hex::encode(id), v.id_hex, "{}: derive_id mismatch", v.label);

    // (2) hash_to_G1: we re-derive Q_id and check by comparing the decryption
    // share for share index 1: D_1 = S_1 · Q_id. If Q_id matches and S_1
    // matches, D_1 matches.

    // Reconstruct shares from Python polynomial coefficients to ensure both
    // sides agree on the secret-share scalars.
    let coeffs: Vec<Fr> = v
        .poly_coeffs_hex
        .iter()
        .map(|h| fr_from_be_hex(h))
        .collect();
    assert_eq!(coeffs.len() as u32, v.threshold);

    let mut rust_shares: Vec<SecretKeyShare> = Vec::with_capacity(v.n as usize);
    for i in 1..=v.n {
        let x = Fr::from(i as u64);
        let mut acc = coeffs[coeffs.len() - 1];
        for k in (0..coeffs.len() - 1).rev() {
            acc = acc * x + coeffs[k];
        }
        rust_shares.push(SecretKeyShare::new(i, acc));
    }
    // Sanity: secret_shares_hex matches our reconstruction.
    for (rust_s, py_s) in rust_shares.iter().zip(v.secret_shares_hex.iter()) {
        assert_eq!(rust_s.index, py_s.index);
        let py_scalar = fr_from_be_hex(&py_s.scalar_hex);
        assert_eq!(
            rust_s.scalar(),
            py_scalar,
            "{}: secret share #{} mismatch",
            v.label,
            rust_s.index
        );
    }

    // (3) Master pub: S · g₂.
    let master_secret = coeffs[0];
    let g2 = G2Affine::generator();
    let master_pub = MasterPublicKey((g2 * master_secret).into_affine());
    assert_eq!(
        g2_compressed_hex(&master_pub.0),
        v.master_pub_compressed_hex,
        "{}: master pub mismatch",
        v.label
    );

    // (4) Share pubs: Sᵢ · g₂.
    for (rust_s, py_pub) in rust_shares.iter().zip(v.share_pubs_compressed_hex.iter()) {
        assert_eq!(rust_s.index, py_pub.index);
        let pt = (g2 * rust_s.scalar()).into_affine();
        assert_eq!(
            g2_compressed_hex(&pt),
            py_pub.bytes_hex,
            "{}: share pub #{} mismatch",
            v.label,
            rust_s.index
        );
    }

    // (5) Decryption shares: Dᵢ = Sᵢ · Q_id, computed via decrypt_share().
    // This implicitly verifies hash_to_G1 because Dᵢ depends on Q_id.
    let envelope = ethexe_tpke::EncryptedEnvelope {
        u: [0u8; 96], // unused for share derivation (decrypt_share only reads id)
        id,
        body: Vec::new(),
    };
    for (rust_s, py_share) in rust_shares
        .iter()
        .zip(v.expected_decryption_shares_hex.iter())
    {
        assert_eq!(rust_s.index, py_share.index);
        let share = rust_s.decrypt_share(&envelope).unwrap();
        assert_eq!(
            g1_compressed_hex(&share.point),
            py_share.bytes_hex,
            "{}: decryption share #{} mismatch \
             (hash_to_G1 or scalar mult diverged from py_ecc reference)",
            v.label,
            rust_s.index
        );
    }

    // (6) Pairing soundness: full Rust roundtrip with the SAME params.
    // If e(Q_id, AggPub)^u == e(D, U), this passes. If pairing direction is
    // swapped or G1/G2 misassigned, decryption fails.
    let share_pubs: Vec<SharePublicKey> = rust_shares
        .iter()
        .map(|s| SharePublicKey {
            index: s.index,
            point: (g2 * s.scalar()).into_affine(),
        })
        .collect();

    // Use a deterministic RNG so this test is reproducible.
    let mut rng = StdRng::seed_from_u64(0xCAFE_0BEEF_u64.wrapping_mul(v.threshold as u64));
    let env = encrypt(
        &master_pub,
        &id,
        v.chain_id,
        v.key_epoch_id,
        &plaintext,
        &mut rng,
    )
    .unwrap();

    // Verify every share.
    let mut shares: Vec<DecryptionShare> = Vec::with_capacity(v.threshold as usize);
    for (i, s) in rust_shares.iter().take(v.threshold as usize).enumerate() {
        let d = s.decrypt_share(&env).unwrap();
        assert!(
            share_pubs[i].verify(&env, &d).unwrap(),
            "{}: share #{} verification failed",
            v.label,
            s.index
        );
        shares.push(d);
    }

    let recovered = combine(&env, &shares, v.chain_id, v.key_epoch_id, v.threshold).unwrap();
    assert_eq!(
        recovered, plaintext,
        "{}: roundtrip decrypt mismatch",
        v.label
    );
}
