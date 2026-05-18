use crate::{
    DealerOutput, DecryptionShare, Encrypted, MasterPublicKey, SharePublicKey, TpkeError, deal,
    decrypt, encrypt,
    utils::{G1_COMPRESSED_LEN, G2_COMPRESSED_LEN},
};
use ark_bls12_381::G2Affine;
use ark_ec::AffineRepr;
use ark_std::rand::{SeedableRng, rngs::StdRng};
use parity_scale_codec::{Decode, Encode};

fn fixed_rng() -> StdRng {
    StdRng::seed_from_u64(0xDEADBEEF_CAFEBABE)
}

fn setup_test<M: Encode>(t: u32, n: u32, message: &M) -> (DealerOutput, Encrypted<M>) {
    let mut rng = fixed_rng();

    let d = deal(t, n, &mut rng).unwrap();
    let enc = encrypt(message, &d.master_pub, &mut rng).unwrap();
    (d, enc)
}

fn decryption_shares<M: Encode + Decode>(
    d: &DealerOutput,
    enc: &Encrypted<M>,
    count: usize,
) -> Vec<DecryptionShare<M>> {
    d.secret_shares
        .iter()
        .take(count)
        .map(|s| s.decrypt_share(enc).unwrap())
        .collect()
}

#[test]
fn roundtrip_4_of_7() {
    let message = String::from("hello, world");
    let (d, enc) = setup_test(4, 7, &message);

    let shares = decryption_shares(&d, &enc, 4);

    // Verify each share.
    for (s, ps) in shares.iter().zip(d.public_shares.iter()) {
        assert!(ps.verify(&enc, s).unwrap());
    }

    let decrypted_message = decrypt(&enc, &shares).unwrap();
    assert_eq!(message, decrypted_message);
}

#[test]
fn insufficient_shares_returns_err() {
    let message = String::from("x");
    let (d, enc) = setup_test(3, 5, &message);

    let shares = decryption_shares(&d, &enc, 2);
    assert!(decrypt(&enc, &shares).is_err());
}

#[test]
fn mutated_ciphertext_fails_aead() {
    let message = String::from("abc");
    let (d, mut enc) = setup_test(2, 3, &message);

    enc.ciphertext.as_mut()[0] ^= 1;

    let shares = decryption_shares(&d, &enc, 2);
    let err = decrypt(&enc, &shares).unwrap_err();
    assert!(matches!(err, TpkeError::Aead(_)));
}

#[test]
fn wrong_hash_fails_aead() {
    let message = String::from("abc");
    let (d, mut enc) = setup_test(2, 3, &message);

    enc.hash = crate::Blake2b256Hash::from(&String::from("wrong"));

    let shares = decryption_shares(&d, &enc, 2);
    let err = decrypt(&enc, &shares).unwrap_err();
    assert!(matches!(err, TpkeError::Aead(_)));
}

#[test]
fn share_for_wrong_validator_fails_verify() {
    let message = String::from("abc");
    let (d, enc) = setup_test(2, 3, &message);

    let mut share = d.secret_shares[0].decrypt_share(&enc).unwrap();
    share.index = 2;

    assert!(!d.public_shares[1].verify(&enc, &share).unwrap());
}

#[test]
fn mutated_hash_breaks_share_verification() {
    let message = String::from("abc");
    let (d, enc) = setup_test(2, 3, &message);

    let share = d.secret_shares[0].decrypt_share(&enc).unwrap();
    let mut tampered = enc.clone();
    tampered.hash = crate::Blake2b256Hash::from(&String::from("wrong"));

    assert!(!d.public_shares[0].verify(&tampered, &share).unwrap());
}

#[test]
fn empty_plaintext_roundtrip() {
    let message = Vec::<u8>::new();
    let (d, enc) = setup_test(2, 3, &message);

    let shares = decryption_shares(&d, &enc, 2);
    let decrypted_message = decrypt(&enc, &shares).unwrap();

    assert_eq!(message, decrypted_message);
}

#[test]
fn duplicate_share_index_rejected() {
    let message = String::from("x");
    let (d, enc) = setup_test(2, 3, &message);

    let share = d.secret_shares[0].decrypt_share(&enc).unwrap();
    let err = decrypt(&enc, &[share.clone(), share]).unwrap_err();

    assert!(matches!(err, TpkeError::DuplicateShareIndex(1)));
}

#[test]
fn invalid_threshold_at_deal_time() {
    let mut rng = fixed_rng();

    assert!(matches!(
        deal(0, 3, &mut rng).unwrap_err(),
        TpkeError::InvalidThreshold { t: 0, n: 3 }
    ));
    assert!(matches!(
        deal(5, 3, &mut rng).unwrap_err(),
        TpkeError::InvalidThreshold { t: 5, n: 3 }
    ));
}

#[test]
fn envelope_scale_roundtrip() {
    let message = String::from("abc");
    let (_, enc) = setup_test(2, 3, &message);

    let encoded = enc.encode();
    let decoded = Encrypted::<String>::decode(&mut &encoded[..]).unwrap();

    assert_eq!(enc, decoded);
}

#[test]
fn decryption_share_scale_roundtrip() {
    let message = String::from("abc");
    let (d, enc) = setup_test(2, 3, &message);

    let share = d.secret_shares[0].decrypt_share(&enc).unwrap();
    let encoded = share.encode();
    let decoded = DecryptionShare::<String>::decode(&mut &encoded[..]).unwrap();

    assert_eq!(encoded.len(), 4 + 32 + G1_COMPRESSED_LEN);
    assert_eq!(share, decoded);
}

#[test]
fn master_pub_scale_roundtrip() {
    let message = String::from("abc");
    let (d, _) = setup_test(2, 3, &message);

    let encoded = d.master_pub.encode();
    let decoded = MasterPublicKey::decode(&mut &encoded[..]).unwrap();

    assert_eq!(encoded.len(), G2_COMPRESSED_LEN);
    assert_eq!(d.master_pub, decoded);
}

#[test]
fn share_pub_scale_roundtrip() {
    let message = String::from("abc");
    let (d, _) = setup_test(2, 3, &message);

    let encoded = d.public_shares[0].encode();
    let decoded = SharePublicKey::decode(&mut &encoded[..]).unwrap();

    assert_eq!(encoded.len(), 4 + G2_COMPRESSED_LEN);
    assert_eq!(d.public_shares[0], decoded);
}

#[test]
fn scale_decode_rejects_identity_master_pub() {
    let encoded = MasterPublicKey(G2Affine::zero()).encode();

    assert!(MasterPublicKey::decode(&mut &encoded[..]).is_err());
}

#[test]
fn arbitrary_subset_of_t_shares_works() {
    let message = String::from("abc");
    let (d, enc) = setup_test(3, 5, &message);
    let all = decryption_shares(&d, &enc, d.secret_shares.len());

    for subset_indices in [[0, 1, 2], [0, 1, 4], [1, 3, 4], [2, 3, 4]] {
        let subset = subset_indices
            .iter()
            .map(|&i| all[i].clone())
            .collect::<Vec<_>>();
        let decrypted_message = decrypt(&enc, &subset).unwrap();
        assert_eq!(
            message, decrypted_message,
            "subset {subset_indices:?} failed"
        );
    }
}

#[test]
fn identity_master_pubkey_rejected_at_encrypt() {
    let identity_pk = MasterPublicKey(G2Affine::zero());
    let mut rng = fixed_rng();
    let message = String::from("x");

    let err = encrypt(&message, &identity_pk, &mut rng).unwrap_err();

    assert!(matches!(err, TpkeError::IdentityPublicKey));
}

#[test]
fn identity_master_pubkey_rejected_at_from_bytes() {
    let bytes = MasterPublicKey(G2Affine::zero()).to_bytes().unwrap();
    let err = MasterPublicKey::from_bytes(&bytes).unwrap_err();

    assert!(matches!(err, TpkeError::IdentityPublicKey));
}

#[test]
fn identity_share_pubkey_rejected_at_from_bytes() {
    let bytes = MasterPublicKey(G2Affine::zero()).to_bytes().unwrap();
    let err = SharePublicKey::from_bytes(1, &bytes).unwrap_err();

    assert!(matches!(err, TpkeError::IdentityPublicKey));
}

#[test]
fn empty_share_set_fails_decrypt() {
    let message = String::from("x");
    let (_, enc) = setup_test(2, 3, &message);

    assert!(decrypt::<String>(&enc, &[]).is_err());
}

#[test]
fn share_from_other_envelope_rejected_in_decrypt() {
    let mut rng = fixed_rng();
    let d = deal(2, 3, &mut rng).unwrap();
    let env_a = encrypt(&String::from("alpha"), &d.master_pub, &mut rng).unwrap();
    let env_b = encrypt(&String::from("beta"), &d.master_pub, &mut rng).unwrap();

    let share_a = d.secret_shares[0].decrypt_share(&env_a).unwrap();
    let share_b = d.secret_shares[1].decrypt_share(&env_b).unwrap();
    let err = decrypt(&env_a, &[share_a, share_b]).unwrap_err();

    assert!(matches!(err, TpkeError::ShareEnvelopeMismatch { index: 2 }));
}

#[test]
fn verify_rejects_wrong_envelope_hash() {
    let mut rng = fixed_rng();
    let d = deal(2, 3, &mut rng).unwrap();
    let env_a = encrypt(&String::from("alpha"), &d.master_pub, &mut rng).unwrap();
    let env_b = encrypt(&String::from("beta"), &d.master_pub, &mut rng).unwrap();

    let share = d.secret_shares[0].decrypt_share(&env_a).unwrap();

    assert!(!d.public_shares[0].verify(&env_b, &share).unwrap());
}

use proptest::prelude::*;

proptest! {
    #![proptest_config(ProptestConfig { cases: 16, .. ProptestConfig::default() })]

    #[test]
    fn proptest_decryption_share_roundtrip(plaintext in proptest::collection::vec(any::<u8>(), 0..200)) {
        let mut rng = fixed_rng();
        let d = deal(2, 3, &mut rng).unwrap();
        let env = encrypt(&plaintext, &d.master_pub, &mut rng).unwrap();
        let share = d.secret_shares[0].decrypt_share(&env).unwrap();

        let (idx, _hash_bytes, point_bytes) = share.to_bytes().unwrap();
        let restored = DecryptionShare::<Vec<u8>>::from_bytes(idx, share.hash, &point_bytes).unwrap();

        prop_assert_eq!(share, restored);
    }

    #[test]
    fn proptest_share_public_key_roundtrip(seed in any::<u64>()) {
        let mut rng = StdRng::seed_from_u64(seed);
        let d = deal(3, 5, &mut rng).unwrap();

        for ps in &d.public_shares {
            let (idx, bytes) = ps.to_bytes().unwrap();
            let restored = SharePublicKey::from_bytes(idx, &bytes).unwrap();
            prop_assert_eq!(ps, &restored);
        }
    }
}
