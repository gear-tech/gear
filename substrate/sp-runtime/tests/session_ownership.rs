// Copyright (C) Gear Technologies Inc.
// SPDX-License-Identifier: Apache-2.0

#![cfg(feature = "std")]

use codec::{DecodeAll, Encode};
use sp_application_crypto::{app_crypto, ecdsa, ed25519, sr25519};
use sp_core::crypto::{key_types, Pair};
use sp_keystore::{testing::MemoryKeystore, KeystoreExt};
use sp_runtime::{impl_opaque_keys, traits::OpaqueKeys, RuntimeAppPublic};

mod babe {
    use super::*;
    app_crypto!(sr25519, key_types::BABE);
}
mod grandpa {
    use super::*;
    app_crypto!(ed25519, key_types::GRANDPA);
}
mod im_online {
    use super::*;
    app_crypto!(sr25519, key_types::IM_ONLINE);
}
mod authority_discovery {
    use super::*;
    app_crypto!(sr25519, key_types::AUTHORITY_DISCOVERY);
}
mod beefy {
    use super::*;
    app_crypto!(ecdsa, key_types::BEEFY);
}

impl_opaque_keys! {
    pub struct SessionKeys {
        pub babe: babe::Public,
        pub grandpa: grandpa::Public,
        pub im_online: im_online::Public,
        pub authority_discovery: authority_discovery::Public,
        pub beefy: beefy::Public,
    }
}
impl_opaque_keys! {
    pub struct OneKey {
        pub babe: babe::Public,
    }
}
impl_opaque_keys! {
    pub struct MockKeys {
        pub key: sp_runtime::testing::UintAuthorityId,
    }
}

type Proof = (
    babe::Signature,
    grandpa::Signature,
    im_online::Signature,
    authority_discovery::Signature,
    beefy::Signature,
);

#[test]
fn five_field_tuple_has_native_wire_contract_and_verifies_every_field() {
    let mut ext = sp_io::TestExternalities::default();
    ext.register_extension(KeystoreExt::new(MemoryKeystore::new()));
    ext.execute_with(|| {
        let owner = [42u8; 32];
        let generated = SessionKeys::generate(&owner, None);
        let keys = generated.keys;
        let proof = generated.proof.encode();
        assert_eq!(keys.encode().len(), 161);
        assert_eq!(proof.len(), 321);
        assert!(keys.ownership_proof_is_valid(&owner, &proof));
        assert!(!keys.ownership_proof_is_valid(&[43; 32], &proof));
        assert!(!keys.ownership_proof_is_valid(&owner.as_slice().encode(), &proof));
        assert!(!keys.ownership_proof_is_valid(b"SS58 owner text", &proof));

        // Verify the wire tuple independently of the OpaqueKeys verifier.
        let decoded = Proof::decode_all(&mut &proof[..]).unwrap();
        let statement = [b"POP_".as_slice(), owner.as_slice()].concat();
        assert!(sr25519::Pair::verify(
            decoded.0.as_ref(),
            &statement,
            keys.babe.as_ref()
        ));
        assert!(ed25519::Pair::verify(
            decoded.1.as_ref(),
            &statement,
            keys.grandpa.as_ref()
        ));
        assert!(sr25519::Pair::verify(
            decoded.2.as_ref(),
            &statement,
            keys.im_online.as_ref()
        ));
        assert!(sr25519::Pair::verify(
            decoded.3.as_ref(),
            &statement,
            keys.authority_discovery.as_ref()
        ));
        assert!(ecdsa::Pair::verify(
            decoded.4.as_ref(),
            &statement,
            keys.beefy.as_ref()
        ));

        for start in [0, 64, 128, 192, 256] {
            let mut corrupted = proof.clone();
            corrupted[start] ^= 1;
            assert!(
                !keys.ownership_proof_is_valid(&owner, &corrupted),
                "field offset {start}"
            );
        }
        for length in 0..proof.len() {
            assert!(!keys.ownership_proof_is_valid(&owner, &proof[..length]));
        }
        let mut trailing = proof.clone();
        trailing.push(0);
        assert!(!keys.ownership_proof_is_valid(&owner, &trailing));

        // Swapping bytes between independently generated sr25519 application keys must fail.
        let mut swapped = proof.clone();
        let (first, rest) = swapped.split_at_mut(128);
        first[..64].swap_with_slice(&mut rest[..64]);
        assert!(!keys.ownership_proof_is_valid(&owner, &swapped));

        let replacement = SessionKeys::generate(&owner, None);
        macro_rules! changed_key {
            ($field:ident) => {
                let mut changed = keys.clone();
                changed.$field = replacement.keys.$field.clone();
                assert!(!changed.ownership_proof_is_valid(&owner, &proof));
            };
        }
        changed_key!(babe);
        changed_key!(grandpa);
        changed_key!(im_online);
        changed_key!(authority_discovery);
        changed_key!(beefy);

        // Native per-key proofs for the same owner can be composed across bundles.
        let mut composed = keys.clone();
        composed.beefy = replacement.keys.beefy;
        let mut composed_proof = generated.proof;
        composed_proof.4 = replacement.proof.4;
        assert!(composed.ownership_proof_is_valid(&owner, &composed_proof.encode()));
        let recreated = composed.create_ownership_proof(&owner).unwrap();
        assert!(composed.ownership_proof_is_valid(&owner, &recreated.encode()));
    });
}

#[test]
fn missing_private_key_fails_instead_of_returning_partial_proof() {
    let mut ext = sp_io::TestExternalities::default();
    ext.register_extension(KeystoreExt::new(MemoryKeystore::new()));
    ext.execute_with(|| {
        let owner = [42u8; 32];
        let mut keys = SessionKeys::generate(&owner, None).keys;
        keys.beefy = ecdsa::Pair::from_seed(&[7; 32]).public().into();
        assert!(keys.create_ownership_proof(&owner).is_err());
    });
}

#[test]
fn single_field_proof_is_a_tuple_and_mock_proofs_bind_the_owner() {
    let mut ext = sp_io::TestExternalities::default();
    ext.register_extension(KeystoreExt::new(MemoryKeystore::new()));
    ext.execute_with(|| {
        let owner = [42u8; 32];
        let generated = OneKey::generate(&owner, Some(b"//Alice".to_vec()));
        let _: (babe::Signature,) = generated.proof.clone();
        assert!(generated
            .keys
            .ownership_proof_is_valid(&owner, &generated.proof.encode()));
        let generated = MockKeys::generate(&owner, None);
        let proof = generated.proof.encode();
        assert!(generated.keys.ownership_proof_is_valid(&owner, &proof));
        assert!(!generated.keys.ownership_proof_is_valid(&[43; 32], &proof));
        let mut key = generated.keys.key;
        let proof = key.generate_proof_of_possession(&owner).unwrap().encode();
        assert!(key.ownership_proof_is_valid(&owner, &proof));
        assert!(!key.ownership_proof_is_valid(&[43; 32], &proof));
        assert!(!key.ownership_proof_is_valid(&owner, &[]));
        let mut trailing = proof;
        trailing.push(0);
        assert!(!key.ownership_proof_is_valid(&owner, &trailing));
    });
}
