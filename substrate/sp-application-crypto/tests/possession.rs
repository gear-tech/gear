// Copyright (C) Gear Technologies Inc.
// SPDX-License-Identifier: Apache-2.0

#![cfg(feature = "std")]

use sp_application_crypto::{ecdsa, ed25519, sr25519, RuntimeAppPublic, RuntimePublic};
use sp_core::{crypto::Pair, U256};
use sp_keystore::{testing::MemoryKeystore, KeystoreExt};

macro_rules! possession_tests {
    ($name:ident, $scheme:ident) => {
        #[test]
        fn $name() {
            let mut ext = sp_io::TestExternalities::default();
            ext.register_extension(KeystoreExt::new(MemoryKeystore::new()));
            ext.execute_with(|| {
                let owner = [42u8; 32];
                let mut public = <$scheme::AppPublic as RuntimeAppPublic>::generate_pair(None);
                let proof = public.generate_proof_of_possession(&owner).unwrap();
                assert!(public.verify_proof_of_possession(&owner, &proof));
                assert!(!public.verify_proof_of_possession(&[43; 32], &proof));

                // Independently verify the exact native domain using the ordinary scheme.
                let statement = [b"POP_".as_slice(), owner.as_slice()].concat();
                assert!($scheme::Pair::verify(
                    proof.as_ref(),
                    &statement,
                    public.as_ref()
                ));
                assert!(!$scheme::Pair::verify(
                    proof.as_ref(),
                    &owner,
                    public.as_ref()
                ));

                let mut bad = proof.clone();
                AsMut::<[u8]>::as_mut(&mut bad)[0] ^= 1;
                assert!(!public.verify_proof_of_possession(&owner, &bad));
                let mut other = <$scheme::AppPublic as RuntimeAppPublic>::generate_pair(None);
                assert!(!other.verify_proof_of_possession(&owner, &proof));
                let other_proof = other.generate_proof_of_possession(&owner).unwrap();
                assert!(other.verify_proof_of_possession(&owner, &other_proof));

                // A normal signature of POP_ || owner is the ownership codec; no bundle hash.
                let pair = $scheme::Pair::from_seed(&[7; 32]);
                let public: $scheme::AppPublic = pair.public().into();
                let independent = pair.sign(&statement).into();
                assert!(public.verify_proof_of_possession(&owner, &independent));
                let wrong_domain = pair.sign(&owner).into();
                assert!(!public.verify_proof_of_possession(&owner, &wrong_domain));
            });

            // An unrelated public key must not produce an empty or invented proof.
            let mut empty = sp_io::TestExternalities::default();
            empty.register_extension(KeystoreExt::new(MemoryKeystore::new()));
            empty.execute_with(|| {
                let mut unknown: $scheme::AppPublic =
                    $scheme::Pair::from_seed(&[7; 32]).public().into();
                assert!(unknown.generate_proof_of_possession(&[42; 32]).is_none());
            });
        }
    };
}

possession_tests!(sr25519_native_possession, sr25519);
possession_tests!(ed25519_native_possession, ed25519);
possession_tests!(ecdsa_native_possession, ecdsa);

#[test]
fn ecdsa_rejects_high_s_and_invalid_scalars_only_for_possession() {
    let pair = ecdsa::Pair::from_seed(&[7; 32]);
    let owner = [42u8; 32];
    let statement = [b"POP_".as_slice(), owner.as_slice()].concat();
    let public = pair.public();
    let proof = pair.sign(&statement);
    assert!(public.verify_proof_of_possession(&owner, &proof));
    let parsed = k256::ecdsa::Signature::try_from(&proof.0[..64]).unwrap();
    assert!(parsed.normalize_s().is_none());

    let order = U256::from_big_endian(&[
        0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff,
        0xfe, 0xba, 0xae, 0xdc, 0xe6, 0xaf, 0x48, 0xa0, 0x3b, 0xbf, 0xd2, 0x5e, 0x8c, 0xd0, 0x36,
        0x41, 0x41,
    ]);
    let mut high_s = proof;
    let s = U256::from_big_endian(&high_s.0[32..64]);
    (order - s).to_big_endian(&mut high_s.0[32..64]);
    high_s.0[64] ^= 1;
    assert!(k256::ecdsa::Signature::try_from(&high_s.0[..64])
        .unwrap()
        .normalize_s()
        .is_some());
    assert!(!public.verify_proof_of_possession(&owner, &high_s));
    // Backport is intentionally not a global ECDSA signature policy change.
    assert!(RuntimePublic::verify(&public, &statement, &high_s));

    for scalar in [0..32, 32..64] {
        let mut invalid = proof;
        invalid.0[scalar.clone()].fill(0);
        assert!(!public.verify_proof_of_possession(&owner, &invalid));
        invalid.0[scalar].fill(0xff);
        assert!(!public.verify_proof_of_possession(&owner, &invalid));
    }
    let mut invalid_recovery = proof;
    invalid_recovery.0[64] = 255;
    assert!(!public.verify_proof_of_possession(&owner, &invalid_recovery));

    // The BEEFY Keccak/prehashed commitment API is not the possession API.
    let prehashed = pair.sign_prehashed(&sp_io::hashing::keccak_256(&statement));
    assert!(!public.verify_proof_of_possession(&owner, &prehashed));
}

#[cfg(feature = "bls-experimental")]
#[test]
fn experimental_possession_is_explicitly_unsupported() {
    macro_rules! unsupported {
        ($scheme:ident) => {
            let pair = sp_application_crypto::$scheme::Pair::from_seed(&[7; 32]);
            let mut public = pair.public();
            let proof = pair.sign(b"POP_owner");
            assert!(public
                .generate_proof_of_possession(sp_core::crypto::KeyTypeId(*b"test"), b"owner")
                .is_none());
            assert!(!public.verify_proof_of_possession(b"owner", &proof));
            let mut app: sp_application_crypto::$scheme::AppPublic = public.into();
            let proof = proof.into();
            assert!(app.generate_proof_of_possession(b"owner").is_none());
            assert!(!app.verify_proof_of_possession(b"owner", &proof));
        };
    }
    unsupported!(bls381);
    unsupported!(ecdsa_bls381);
}
