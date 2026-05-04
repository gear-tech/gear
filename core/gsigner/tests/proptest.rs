// This file is part of Gear.
//
// Copyright (C) 2021-2026 Gear Technologies Inc.
// SPDX-License-Identifier: GPL-3.0-or-later WITH Classpath-exception-2.0
//
// This program is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.
//
// This program is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE. See the
// GNU General Public License for more details.
//
// You should have received a copy of the GNU General Public License
// along with this program. If not, see <https://www.gnu.org/licenses/>.

//! Property-based tests for gsigner using proptest.
//!
//! These tests verify invariants that should hold for any input.
//! Test case counts are kept small for faster CI runs.

#![cfg(feature = "std")]

use proptest::prelude::*;

// Configure proptest to run fewer cases for faster execution
fn config() -> ProptestConfig {
    ProptestConfig {
        cases: 10,
        ..ProptestConfig::default()
    }
}

// =============================================================================
// secp256k1 property tests
// =============================================================================

#[cfg(feature = "secp256k1")]
mod secp256k1_props {
    use super::*;
    use gsigner::{
        scheme::CryptoScheme,
        schemes::secp256k1::{Digest, PrivateKey, Secp256k1, Signature},
    };

    proptest! {
        #![proptest_config(config())]

        /// Any data should sign and verify successfully
        #[test]
        fn sign_verify_roundtrip(data in prop::collection::vec(any::<u8>(), 0..500)) {
            let private = PrivateKey::random();
            let public = private.public_key();

            let signature = Secp256k1::sign(&private, &data).unwrap();
            Secp256k1::verify(&public, &data, &signature).unwrap();
        }

        /// Tampered data should fail verification
        #[test]
        fn tampered_data_fails_verification(
            data in prop::collection::vec(any::<u8>(), 1..100),
            tamper_idx in 0usize..100,
        ) {
            let private = PrivateKey::random();
            let public = private.public_key();

            let signature = Secp256k1::sign(&private, &data).unwrap();

            let idx = tamper_idx % data.len();
            let mut tampered = data.clone();
            tampered[idx] = tampered[idx].wrapping_add(1);

            prop_assert!(Secp256k1::verify(&public, &tampered, &signature).is_err());
        }

        /// Signature recovery should return correct public key
        #[test]
        fn signature_recovery_returns_correct_key(data in prop::collection::vec(any::<u8>(), 1..100)) {
            let private = PrivateKey::random();
            let public = private.public_key();

            let signature = Signature::create(&private, &data).unwrap();
            let digest = Digest::from(data.as_slice());
            let recovered = signature.validate(&digest).unwrap();

            prop_assert_eq!(recovered, public);
        }
    }
}

// =============================================================================
// ed25519 property tests
// =============================================================================

#[cfg(feature = "ed25519")]
mod ed25519_props {
    use super::*;
    use gsigner::{
        scheme::CryptoScheme,
        schemes::ed25519::{Ed25519, PrivateKey},
    };

    proptest! {
        #![proptest_config(config())]

        /// Any data should sign and verify successfully
        #[test]
        fn sign_verify_roundtrip(data in prop::collection::vec(any::<u8>(), 0..500)) {
            let private = PrivateKey::random();
            let public = private.public_key();

            let signature = Ed25519::sign(&private, &data).unwrap();
            Ed25519::verify(&public, &data, &signature).unwrap();
        }

        /// Tampered data should fail verification
        #[test]
        fn tampered_data_fails_verification(
            data in prop::collection::vec(any::<u8>(), 1..100),
            tamper_idx in 0usize..100,
        ) {
            let private = PrivateKey::random();
            let public = private.public_key();

            let signature = Ed25519::sign(&private, &data).unwrap();

            let idx = tamper_idx % data.len();
            let mut tampered = data.clone();
            tampered[idx] = tampered[idx].wrapping_add(1);

            prop_assert!(Ed25519::verify(&public, &tampered, &signature).is_err());
        }

        /// ed25519 signatures should be deterministic (same key, same data = same sig)
        #[test]
        fn ed25519_signatures_are_deterministic(data in prop::collection::vec(any::<u8>(), 1..100)) {
            let private = PrivateKey::random();

            let sig1 = Ed25519::sign(&private, &data).unwrap();
            let sig2 = Ed25519::sign(&private, &data).unwrap();

            prop_assert_eq!(sig1.to_bytes(), sig2.to_bytes());
        }
    }
}

// =============================================================================
// sr25519 property tests
// =============================================================================

#[cfg(feature = "sr25519")]
mod sr25519_props {
    use super::*;
    use gsigner::{
        scheme::CryptoScheme,
        schemes::sr25519::{PrivateKey, Sr25519},
    };

    proptest! {
        #![proptest_config(config())]

        /// Any data should sign and verify successfully
        #[test]
        fn sign_verify_roundtrip(data in prop::collection::vec(any::<u8>(), 0..500)) {
            let private = PrivateKey::random();
            let public = private.public_key();

            let signature = Sr25519::sign(&private, &data).unwrap();
            Sr25519::verify(&public, &data, &signature).unwrap();
        }

        /// Tampered data should fail verification
        #[test]
        fn tampered_data_fails_verification(
            data in prop::collection::vec(any::<u8>(), 1..100),
            tamper_idx in 0usize..100,
        ) {
            let private = PrivateKey::random();
            let public = private.public_key();

            let signature = Sr25519::sign(&private, &data).unwrap();

            let idx = tamper_idx % data.len();
            let mut tampered = data.clone();
            tampered[idx] = tampered[idx].wrapping_add(1);

            prop_assert!(Sr25519::verify(&public, &tampered, &signature).is_err());
        }
    }
}

// =============================================================================
// Storage backend property tests
// =============================================================================

#[cfg(all(feature = "keyring", feature = "secp256k1"))]
mod storage_props {
    use super::*;
    use gsigner::{schemes::secp256k1::Secp256k1, signer::Signer};

    proptest! {
        #![proptest_config(config())]

        /// Any generated key should be retrievable
        #[test]
        fn generated_keys_are_retrievable(key_count in 1usize..5) {
            let signer = Signer::<Secp256k1>::memory();

            let keys: Vec<_> = (0..key_count)
                .map(|_| signer.generate().unwrap())
                .collect();

            for key in &keys {
                prop_assert!(signer.has_key(*key).unwrap());
            }

            let listed = signer.list_keys().unwrap();
            prop_assert_eq!(listed.len(), key_count);
        }

        /// Sign then verify should always succeed for valid keys
        #[test]
        fn sign_verify_with_signer(data in prop::collection::vec(any::<u8>(), 0..200)) {
            let signer = Signer::<Secp256k1>::memory();
            let public = signer.generate().unwrap();

            let signature = signer.sign(public, &data).unwrap();
            signer.verify(public, &data, &signature).unwrap();
        }
    }
}
