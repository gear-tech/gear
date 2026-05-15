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

//! Known test vector tests for gsigner.
//!
//! These tests verify that our implementations produce outputs
//! matching known good values from reference implementations.

#![cfg(feature = "std")]

// =============================================================================
// secp256k1 / Ethereum test vectors
// =============================================================================

#[cfg(feature = "secp256k1")]
mod secp256k1_vectors {
    use gsigner::schemes::secp256k1::PrivateKey;

    /// Test vector from go-ethereum / web3.js documentation
    /// Private key: 0x4c0883a69102937d6231471b5dbb6204fe5129617082792ae468d01a3f362318
    /// Address: 0x2c7536E3605D9C16a7a3D7b1898e529396a65c23
    #[test]
    fn test_ethereum_address_derivation() {
        let private_hex = "4c0883a69102937d6231471b5dbb6204fe5129617082792ae468d01a3f362318";
        let expected_address = "2c7536e3605d9c16a7a3d7b1898e529396a65c23";

        let private_bytes = hex::decode(private_hex).unwrap();
        let mut seed = [0u8; 32];
        seed.copy_from_slice(&private_bytes);
        let private = PrivateKey::from_pair_seed(seed);
        let public = private.public_key();
        let address = public.to_address();

        assert_eq!(address.to_hex().to_lowercase(), expected_address);
    }
}

// =============================================================================
// ed25519 test vectors (RFC 8032)
// =============================================================================

#[cfg(feature = "ed25519")]
mod ed25519_vectors {
    use gsigner::schemes::ed25519::PrivateKey;

    /// RFC 8032 Test Vector 1 (empty message)
    /// Secret key: 9d61b19deffd5a60ba844af492ec2cc44449c5697b326919703bac031cae7f60
    /// Public key: d75a980182b10ab7d54bfed3c964073a0ee172f3daa62325af021a68f707511a
    #[test]
    fn test_rfc8032_vector1_public_key_derivation() {
        let secret_hex = "9d61b19deffd5a60ba844af492ec2cc44449c5697b326919703bac031cae7f60";
        let expected_public_hex =
            "d75a980182b10ab7d54bfed3c964073a0ee172f3daa62325af021a68f707511a";

        let secret_bytes = hex::decode(secret_hex).unwrap();
        let mut seed = [0u8; 32];
        seed.copy_from_slice(&secret_bytes);

        let private = PrivateKey::from_seed(seed).unwrap();
        let public = private.public_key();

        assert_eq!(public.to_hex().to_lowercase(), expected_public_hex);
    }
}

// =============================================================================
// sr25519 test vectors (Substrate/polkadot-js compatibility)
// =============================================================================

#[cfg(feature = "sr25519")]
mod sr25519_vectors {
    use gsigner::{scheme::CryptoScheme, schemes::sr25519::PrivateKey};

    /// Test that sr25519 signatures are non-deterministic (due to random nonce)
    /// This is a key property that distinguishes sr25519 from ed25519
    #[test]
    fn test_sr25519_signatures_are_randomized() {
        use gsigner::schemes::sr25519::Sr25519;

        let private = PrivateKey::random();
        let message = b"same message";

        let sig1 = Sr25519::sign(&private, message).unwrap();
        let sig2 = Sr25519::sign(&private, message).unwrap();

        // Signatures should be different (sr25519 uses random nonce)
        assert_ne!(sig1.to_bytes(), sig2.to_bytes());

        // But both should verify
        let public = private.public_key();
        Sr25519::verify(&public, message, &sig1).unwrap();
        Sr25519::verify(&public, message, &sig2).unwrap();
    }

    /// Test mnemonic import produces deterministic keys
    #[test]
    fn test_mnemonic_import() {
        // Standard BIP-39 test mnemonic
        let phrase = "bottom drive obey lake curtain smoke basket hold race lonely fit walk";
        let key1 = PrivateKey::from_phrase(phrase, None).unwrap();
        let key2 = PrivateKey::from_phrase(phrase, None).unwrap();

        // Same phrase should produce same key
        assert_eq!(key1.public_key(), key2.public_key());
    }
}

// =============================================================================
// Cross-scheme tests
// =============================================================================

#[cfg(all(feature = "secp256k1", feature = "ed25519", feature = "sr25519"))]
mod cross_scheme {
    /// Verify that all schemes can sign empty messages
    #[test]
    fn test_all_schemes_sign_empty() {
        use gsigner::scheme::CryptoScheme;

        // secp256k1
        {
            use gsigner::schemes::secp256k1::{PrivateKey, Secp256k1};
            let private = PrivateKey::random();
            let public = private.public_key();
            let sig = Secp256k1::sign(&private, b"").unwrap();
            Secp256k1::verify(&public, b"", &sig).unwrap();
        }

        // ed25519
        {
            use gsigner::schemes::ed25519::{Ed25519, PrivateKey};
            let private = PrivateKey::random();
            let public = private.public_key();
            let sig = Ed25519::sign(&private, b"").unwrap();
            Ed25519::verify(&public, b"", &sig).unwrap();
        }

        // sr25519
        {
            use gsigner::schemes::sr25519::{PrivateKey, Sr25519};
            let private = PrivateKey::random();
            let public = private.public_key();
            let sig = Sr25519::sign(&private, b"").unwrap();
            Sr25519::verify(&public, b"", &sig).unwrap();
        }
    }

    /// Verify that all schemes handle large messages
    #[test]
    fn test_all_schemes_large_message() {
        use gsigner::scheme::CryptoScheme;
        let large_msg = vec![0xABu8; 100_000];

        // secp256k1
        {
            use gsigner::schemes::secp256k1::{PrivateKey, Secp256k1};
            let private = PrivateKey::random();
            let public = private.public_key();
            let sig = Secp256k1::sign(&private, &large_msg).unwrap();
            Secp256k1::verify(&public, &large_msg, &sig).unwrap();
        }

        // ed25519
        {
            use gsigner::schemes::ed25519::{Ed25519, PrivateKey};
            let private = PrivateKey::random();
            let public = private.public_key();
            let sig = Ed25519::sign(&private, &large_msg).unwrap();
            Ed25519::verify(&public, &large_msg, &sig).unwrap();
        }

        // sr25519
        {
            use gsigner::schemes::sr25519::{PrivateKey, Sr25519};
            let private = PrivateKey::random();
            let public = private.public_key();
            let sig = Sr25519::sign(&private, &large_msg).unwrap();
            Sr25519::verify(&public, &large_msg, &sig).unwrap();
        }
    }
}
