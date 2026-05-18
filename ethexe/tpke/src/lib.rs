// This file is part of Gear.
//
// Copyright (C) 2026 Gear Technologies Inc.
// SPDX-License-Identifier: GPL-3.0-or-later WITH Classpath-exception-2.0

//! Threshold public-key encryption for ethexe private transactions.
//!
//! Construction: Boneh-Franklin identity-based TPKE on BLS12-381, with the
//! master secret split into n Shamir shares (threshold t). Encryption is
//! identity-bound: every ciphertext carries an `id` and a decryption share
//! produced for `id` only decrypts that one ciphertext.
//!
//! Pairing orientation (Type-3 on BLS12-381):
//!   - `Q_id ∈ G1` via hash-to-curve (DST below)
//!   - master pubkey, share pubkeys, ephemeral U  ∈ G2
//!   - decryption shares                          ∈ G1
//!   - e: G1 × G2 → GT
//!
//! IND-CCA via ChaCha20-Poly1305 (KEM/DEM with HKDF-SHA256 key/nonce derivation).
//! The DEM AAD binds (id, U_bytes, chain_id, key_epoch_id) into the MAC.

#![cfg_attr(not(feature = "std"), no_std)]

#[cfg(not(feature = "std"))]
extern crate alloc;

mod aead;
mod bls12_381;
mod keys;
mod shamir;

mod r#trait;
pub use r#trait::Encryptable;

pub use aead::HKDF_DEM_INFO;
pub use bls12_381::{DST_G1, G1_COMPRESSED_LEN, G2_COMPRESSED_LEN, ID_DOMAIN, combine};
pub use keys::{
    DealerOutput, DecryptionShare, Encrypted, MasterPublicKey, MasterSecretKey, SecretKeyShare,
    SharePublicKey,
};

#[derive(thiserror::Error, Debug, Clone, PartialEq, Eq)]
pub enum TpkeError {
    #[error("malformed ciphertext envelope")]
    MalformedCiphertext,
    #[error("AEAD authentication failed")]
    AeadAuth,
    #[error("decryption share did not verify against share public key")]
    ShareVerification,
    #[error("not enough shares to combine: got {got}, need {need}")]
    InsufficientShares { got: usize, need: usize },
    #[error("duplicate share index {0}")]
    DuplicateShareIndex(u32),
    #[error("share index {0} is zero (validator ids start at 1)")]
    ZeroShareIndex(u32),
    #[error("share #{index} bound to a different envelope id than the target")]
    ShareEnvelopeMismatch { index: u32 },
    #[error("point serialization failed")]
    Serialization,
    #[error("hash-to-curve failed")]
    HashToCurve,
    #[error("invalid threshold: t={t}, n={n} (require 1 <= t <= n)")]
    InvalidThreshold { t: u32, n: u32 },
    #[error("public key is the identity point — refusing to use it")]
    IdentityPublicKey,
    #[error("payload decode failed: {0}")]
    PayloadDecode(parity_scale_codec::Error),
}

pub type TpkeResult<T> = Result<T, TpkeError>;

#[cfg(test)]
mod tests {
    use super::*;
    use ark_std::rand::{SeedableRng, rngs::StdRng};

    fn fixed_rng() -> StdRng {
        StdRng::seed_from_u64(0xDEADBEEF_CAFEBABE)
    }

    fn deal(t: u32, n: u32) -> DealerOutput {
        let mut rng = fixed_rng();
        MasterSecretKey::deal(t, n, &mut rng).unwrap()
    }

    impl Encryptable for String {
        type Id = [u8; 32];
        type Payload = Self;

        fn payload(&self) -> &Self::Payload {
            &self
        }

        fn tpke_id(&self) -> Self::Id {
            return [0u8; 32];
        }
    }

    #[test]
    fn roundtrip_4_of_7() {
        let d = deal(4, 7);
        let mut rng = fixed_rng();
        let text = String::from("hello world");
        let env = text.encrypt(&d.master_pub, &mut rng).unwrap();
        let shares = d
            .shares
            .iter()
            .take(4)
            .map(|s| s.decrypt_share(&env).unwrap())
            .collect::<Vec<_>>();

        // Verify each share.
        for (s, ps) in shares.iter().zip(d.share_pubs.iter()) {
            assert!(ps.verify(&env, s).unwrap());
        }
        let pt = combine(&env, &shares, 4).unwrap();
        assert_eq!(pt, String::from("hello world"));
    }

    // #[test]
    // fn insufficient_shares_returns_err() {
    //     let d = deal(3, 5);
    //     let mut rng = fixed_rng();
    //     let id = derive_id(1, 0, b"x", &[0u8; 32]);
    //     let env = encrypt(&d.master_pub, &id, 1, 0, b"x", &mut rng).unwrap();
    //     let shares: Vec<_> = d
    //         .shares
    //         .iter()
    //         .take(2)
    //         .map(|s| s.decrypt_share(&env).unwrap())
    //         .collect();
    //     let err = combine(&env, &shares, 1, 0, 3).unwrap_err();
    //     assert!(matches!(
    //         err,
    //         TpkeError::InsufficientShares { got: 2, need: 3 }
    //     ));
    // }

    // #[test]
    // fn mutated_ciphertext_fails_aead() {
    //     let d = deal(2, 3);
    //     let mut rng = fixed_rng();
    //     let id = derive_id(1, 0, b"abc", &[1u8; 32]);
    //     let mut env = encrypt(&d.master_pub, &id, 1, 0, b"abc", &mut rng).unwrap();
    //     // Flip one bit of the ciphertext body.
    //     env.body[0] ^= 1;
    //     let shares: Vec<_> = d
    //         .shares
    //         .iter()
    //         .take(2)
    //         .map(|s| s.decrypt_share(&env).unwrap())
    //         .collect();
    //     let err = combine(&env, &shares, 1, 0, 2).unwrap_err();
    //     assert!(matches!(err, TpkeError::AeadAuth));
    // }

    // #[test]
    // fn wrong_aad_chain_id_fails_aead() {
    //     let d = deal(2, 3);
    //     let mut rng = fixed_rng();
    //     let id = derive_id(1, 0, b"abc", &[1u8; 32]);
    //     let env = encrypt(&d.master_pub, &id, 1, 0, b"abc", &mut rng).unwrap();
    //     let shares: Vec<_> = d
    //         .shares
    //         .iter()
    //         .take(2)
    //         .map(|s| s.decrypt_share(&env).unwrap())
    //         .collect();
    //     // Decrypt with wrong chain_id.
    //     let err = combine(&env, &shares, 999, 0, 2).unwrap_err();
    //     assert!(matches!(err, TpkeError::AeadAuth));
    // }

    // #[test]
    // fn share_for_wrong_validator_fails_verify() {
    //     let d = deal(2, 3);
    //     let mut rng = fixed_rng();
    //     let id = derive_id(1, 0, b"abc", &[1u8; 32]);
    //     let env = encrypt(&d.master_pub, &id, 1, 0, b"abc", &mut rng).unwrap();
    //     // Validator 1 produces a share but we check it against validator 2's pubkey.
    //     let share1 = d.shares[0].decrypt_share(&env).unwrap();
    //     // Swap index to 2 to bypass the index-mismatch shortcut and trigger the pairing check.
    //     let mut fake = share1.clone();
    //     fake.index = 2;
    //     assert!(!d.share_pubs[1].verify(&env, &fake).unwrap());
    // }

    // #[test]
    // fn mutated_id_breaks_share_verification() {
    //     let d = deal(2, 3);
    //     let mut rng = fixed_rng();
    //     let id = derive_id(1, 0, b"abc", &[1u8; 32]);
    //     let env = encrypt(&d.master_pub, &id, 1, 0, b"abc", &mut rng).unwrap();
    //     let share = d.shares[0].decrypt_share(&env).unwrap();
    //     let mut tampered = env.clone();
    //     tampered.id[0] ^= 0xFF;
    //     // Share was for original id, not the tampered one: pairing check fails.
    //     assert!(!d.share_pubs[0].verify(&tampered, &share).unwrap());
    // }

    // #[test]
    // fn empty_plaintext_roundtrip() {
    //     let d = deal(2, 3);
    //     let mut rng = fixed_rng();
    //     let id = derive_id(1, 0, b"", &[2u8; 32]);
    //     let env = encrypt(&d.master_pub, &id, 1, 0, b"", &mut rng).unwrap();
    //     let shares: Vec<_> = d
    //         .shares
    //         .iter()
    //         .take(2)
    //         .map(|s| s.decrypt_share(&env).unwrap())
    //         .collect();
    //     let pt = combine(&env, &shares, 1, 0, 2).unwrap();
    //     assert_eq!(pt, b"");
    // }

    // #[test]
    // fn duplicate_share_index_rejected() {
    //     let d = deal(2, 3);
    //     let mut rng = fixed_rng();
    //     let id = derive_id(1, 0, b"x", &[3u8; 32]);
    //     let env = encrypt(&d.master_pub, &id, 1, 0, b"x", &mut rng).unwrap();
    //     let share = d.shares[0].decrypt_share(&env).unwrap();
    //     // Same share twice.
    //     let err = combine(&env, &[share.clone(), share.clone()], 1, 0, 2).unwrap_err();
    //     assert!(matches!(err, TpkeError::DuplicateShareIndex(1)));
    // }

    // #[test]
    // fn invalid_threshold_at_deal_time() {
    //     let mut rng = fixed_rng();
    //     assert!(matches!(
    //         MasterSecretKey::deal(0, 3, &mut rng).unwrap_err(),
    //         TpkeError::InvalidThreshold { t: 0, n: 3 }
    //     ));
    //     assert!(matches!(
    //         MasterSecretKey::deal(5, 3, &mut rng).unwrap_err(),
    //         TpkeError::InvalidThreshold { t: 5, n: 3 }
    //     ));
    // }

    // #[test]
    // fn id_derivation_is_deterministic_and_nonce_sensitive() {
    //     let a = derive_id(1, 0, b"hello", &[0u8; 32]);
    //     let b = derive_id(1, 0, b"hello", &[0u8; 32]);
    //     let c = derive_id(1, 0, b"hello", &[1u8; 32]);
    //     let d = derive_id(2, 0, b"hello", &[0u8; 32]);
    //     let e = derive_id(1, 1, b"hello", &[0u8; 32]);
    //     assert_eq!(a, b);
    //     assert_ne!(a, c, "different nonce must give different id");
    //     assert_ne!(a, d, "different chain_id must give different id");
    //     assert_ne!(a, e, "different key_epoch_id must give different id");
    // }

    // #[test]
    // fn envelope_scale_roundtrip() {
    //     let d = deal(2, 3);
    //     let mut rng = fixed_rng();
    //     let id = derive_id(1, 0, b"abc", &[4u8; 32]);
    //     let env = encrypt(&d.master_pub, &id, 1, 0, b"abc", &mut rng).unwrap();
    //     let encoded = env.encode();
    //     let decoded = EncryptedEnvelope::decode(&mut &encoded[..]).unwrap();
    //     assert_eq!(env, decoded);
    // }

    // #[test]
    // fn decryption_share_scale_roundtrip() {
    //     let d = deal(2, 3);
    //     let mut rng = fixed_rng();
    //     let id = derive_id(1, 0, b"abc", &[5u8; 32]);
    //     let env = encrypt(&d.master_pub, &id, 1, 0, b"abc", &mut rng).unwrap();
    //     let share = d.shares[0].decrypt_share(&env).unwrap();
    //     let encoded = share.encode();
    //     // 4 (index) + 32 (id) + 48 (G1 compressed) = 84 bytes.
    //     assert_eq!(encoded.len(), 4 + 32 + 48);
    //     let decoded = DecryptionShare::decode(&mut &encoded[..]).unwrap();
    //     assert_eq!(share, decoded);
    // }

    // #[test]
    // fn master_pub_scale_roundtrip() {
    //     let d = deal(2, 3);
    //     let encoded = d.master_pub.encode();
    //     assert_eq!(encoded.len(), G2_COMPRESSED_LEN);
    //     let decoded = MasterPublicKey::decode(&mut &encoded[..]).unwrap();
    //     assert_eq!(d.master_pub, decoded);
    // }

    // #[test]
    // fn share_pub_scale_roundtrip() {
    //     let d = deal(2, 3);
    //     let encoded = d.share_pubs[0].encode();
    //     assert_eq!(encoded.len(), 4 + G2_COMPRESSED_LEN);
    //     let decoded = SharePublicKey::decode(&mut &encoded[..]).unwrap();
    //     assert_eq!(d.share_pubs[0], decoded);
    // }

    // #[test]
    // fn scale_decode_rejects_identity_master_pub() {
    //     // Serialize the G2 identity and try to SCALE-decode as a MasterPublicKey.
    //     let mut buf = [0u8; G2_COMPRESSED_LEN];
    //     G2Affine::zero().serialize_compressed(&mut buf[..]).unwrap();
    //     let encoded = buf.encode();
    //     assert!(MasterPublicKey::decode(&mut &encoded[..]).is_err());
    // }

    // #[test]
    // fn arbitrary_subset_of_t_shares_works() {
    //     // 3-of-5: any 3 distinct shares must combine to the same plaintext.
    //     let d = deal(3, 5);
    //     let mut rng = fixed_rng();
    //     let id = derive_id(1, 0, b"abc", &[5u8; 32]);
    //     let env = encrypt(&d.master_pub, &id, 1, 0, b"abc", &mut rng).unwrap();
    //     let all: Vec<_> = d
    //         .shares
    //         .iter()
    //         .map(|s| s.decrypt_share(&env).unwrap())
    //         .collect();
    //     for subset_indices in [[0, 1, 2], [0, 1, 4], [1, 3, 4], [2, 3, 4]] {
    //         let subset: Vec<_> = subset_indices.iter().map(|&i| all[i].clone()).collect();
    //         let pt = combine(&env, &subset, 1, 0, 3).unwrap();
    //         assert_eq!(pt, b"abc", "subset {subset_indices:?} failed");
    //     }
    // }

    // #[test]
    // fn take_master_secret_is_one_shot() {
    //     let mut d = deal(2, 3);
    //     assert!(d.take_master_secret().is_some());
    //     assert!(d.take_master_secret().is_none());
    //     // Subsequent state is otherwise intact.
    //     assert_eq!(d.shares.len(), 3);
    // }

    // // Regression tests for codex review findings (PR gear-tech/gear#5427).
    // //
    // // [P1] An identity G2 master pubkey makes `e(Q_id, pk) = 1_GT`, letting
    // // anyone derive the DEM key from the public envelope alone. Encryption
    // // and pubkey deserialization must reject it.
    // //
    // // [P2] `combine(.., threshold=0)` would slice to an empty share set and
    // // interpolate to the G1 identity, producing a usable D under an attacker-
    // // controlled identity master pubkey. Reject it to mirror deal()'s rule.

    // #[test]
    // fn identity_master_pubkey_rejected_at_encrypt() {
    //     let identity_pk = MasterPublicKey(G2Affine::zero());
    //     let mut rng = fixed_rng();
    //     let id = derive_id(1, 0, b"x", &[0u8; 32]);
    //     let err = encrypt(&identity_pk, &id, 1, 0, b"x", &mut rng).unwrap_err();
    //     assert!(matches!(err, TpkeError::IdentityPublicKey));
    // }

    // #[test]
    // fn identity_master_pubkey_rejected_at_from_bytes() {
    //     let mut buf = [0u8; G2_COMPRESSED_LEN];
    //     G2Affine::zero().serialize_compressed(&mut buf[..]).unwrap();
    //     let err = MasterPublicKey::from_bytes(&buf).unwrap_err();
    //     assert!(matches!(err, TpkeError::IdentityPublicKey));
    // }

    // #[test]
    // fn identity_share_pubkey_rejected_at_from_bytes() {
    //     let mut buf = [0u8; G2_COMPRESSED_LEN];
    //     G2Affine::zero().serialize_compressed(&mut buf[..]).unwrap();
    //     let err = SharePublicKey::from_bytes(1, &buf).unwrap_err();
    //     assert!(matches!(err, TpkeError::IdentityPublicKey));
    // }

    // #[test]
    // fn zero_threshold_rejected_in_combine() {
    //     let d = deal(2, 3);
    //     let mut rng = fixed_rng();
    //     let id = derive_id(1, 0, b"x", &[0u8; 32]);
    //     let env = encrypt(&d.master_pub, &id, 1, 0, b"x", &mut rng).unwrap();
    //     let err = combine(&env, &[], 1, 0, 0).unwrap_err();
    //     assert!(matches!(err, TpkeError::InvalidThreshold { t: 0, n: 0 }));
    // }

    // #[test]
    // fn share_from_other_envelope_rejected_in_combine() {
    //     let d = deal(2, 3);
    //     let mut rng = fixed_rng();
    //     let id_a = derive_id(1, 0, b"alpha", &[0xAAu8; 32]);
    //     let id_b = derive_id(1, 0, b"beta", &[0xBBu8; 32]);
    //     let env_a = encrypt(&d.master_pub, &id_a, 1, 0, b"alpha", &mut rng).unwrap();
    //     let env_b = encrypt(&d.master_pub, &id_b, 1, 0, b"beta", &mut rng).unwrap();
    //     // Take share #1 from envelope A and share #2 from envelope B.
    //     let s_a = d.shares[0].decrypt_share(&env_a).unwrap();
    //     let s_b = d.shares[1].decrypt_share(&env_b).unwrap();
    //     // Try to combine for envelope A — share #2 has the wrong id.
    //     let err = combine(&env_a, &[s_a, s_b], 1, 0, 2).unwrap_err();
    //     assert!(matches!(err, TpkeError::ShareEnvelopeMismatch { index: 2 }));
    // }

    // #[test]
    // fn verify_rejects_wrong_envelope_id() {
    //     let d = deal(2, 3);
    //     let mut rng = fixed_rng();
    //     let id_a = derive_id(1, 0, b"alpha", &[0xAAu8; 32]);
    //     let id_b = derive_id(1, 0, b"beta", &[0xBBu8; 32]);
    //     let env_a = encrypt(&d.master_pub, &id_a, 1, 0, b"alpha", &mut rng).unwrap();
    //     let env_b = encrypt(&d.master_pub, &id_b, 1, 0, b"beta", &mut rng).unwrap();
    //     // Share is for env_a but we verify against env_b — must return false.
    //     let share = d.shares[0].decrypt_share(&env_a).unwrap();
    //     assert!(!d.share_pubs[0].verify(&env_b, &share).unwrap());
    // }

    // // ----- to_bytes / from_bytes roundtrip property tests -----

    // use proptest::prelude::*;

    // proptest! {
    //     #![proptest_config(ProptestConfig { cases: 16, .. ProptestConfig::default() })]

    //     #[test]
    //     fn proptest_decryption_share_roundtrip(plaintext in proptest::collection::vec(any::<u8>(), 0..200)) {
    //         let d = deal(2, 3);
    //         let mut rng = fixed_rng();
    //         let id = derive_id(1, 0, &plaintext, &[7u8; 32]);
    //         let env = encrypt(&d.master_pub, &id, &plaintext, &mut rng).unwrap();
    //         let share = d.shares[0].decrypt_share(&env).unwrap();
    //         let (idx, id_bytes, point_bytes) = share.to_bytes().unwrap();
    //         let restored = DecryptionShare::from_bytes(idx, id_bytes, &point_bytes).unwrap();
    //         prop_assert_eq!(share, restored);
    //     }

    //     #[test]
    //     fn proptest_master_public_key_roundtrip(seed in any::<u64>()) {
    //         let mut rng = StdRng::seed_from_u64(seed);
    //         let mut d = MasterSecretKey::deal(2, 3, &mut rng).unwrap();
    //         let _ = d.take_master_secret();
    //         let bytes = d.master_pub.to_bytes().unwrap();
    //         let restored = MasterPublicKey::from_bytes(&bytes).unwrap();
    //         prop_assert_eq!(d.master_pub, restored);
    //     }

    //     #[test]
    //     fn proptest_share_public_key_roundtrip(seed in any::<u64>()) {
    //         let mut rng = StdRng::seed_from_u64(seed);
    //         let d = MasterSecretKey::deal(3, 5, &mut rng).unwrap();
    //         for ps in &d.share_pubs {
    //             let (idx, bytes) = ps.to_bytes().unwrap();
    //             let restored = SharePublicKey::from_bytes(idx, &bytes).unwrap();
    //             prop_assert_eq!(ps, &restored);
    //         }
    //     }
    // }
}
