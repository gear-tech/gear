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

//! Integration tests for gsigner.
//!
//! These tests verify cross-scheme consistency, full workflows,
//! error handling, and known test vectors.

#![cfg(all(feature = "std", feature = "keyring", feature = "serde"))]

use gsigner::{keyring::KeyringScheme, signer::Signer};

// =============================================================================
// Cross-scheme consistency tests
// =============================================================================

/// Helper to test that a scheme works correctly through the Signer API.
fn assert_scheme_works<S: KeyringScheme>() {
    let signer = Signer::<S>::memory();

    // Generate key
    let public = signer.generate().unwrap();
    assert!(signer.has_key(public.clone()).unwrap());

    // Sign and verify
    let message = b"test message for signing";
    let signature = signer.sign(public.clone(), message).unwrap();
    signer.verify(public.clone(), message, &signature).unwrap();

    // List keys
    let keys = signer.list_keys().unwrap();
    assert!(keys.contains(&public));

    // Clear keys
    signer.clear_keys().unwrap();
    assert!(!signer.has_key(public).unwrap());
}

#[cfg(feature = "secp256k1")]
#[test]
fn test_secp256k1_scheme_works() {
    assert_scheme_works::<gsigner::schemes::secp256k1::Secp256k1>();
}

#[cfg(feature = "ed25519")]
#[test]
fn test_ed25519_scheme_works() {
    assert_scheme_works::<gsigner::schemes::ed25519::Ed25519>();
}

#[cfg(feature = "sr25519")]
#[test]
fn test_sr25519_scheme_works() {
    assert_scheme_works::<gsigner::schemes::sr25519::Sr25519>();
}

// =============================================================================
// Full workflow tests
// =============================================================================

#[cfg(feature = "secp256k1")]
#[test]
fn test_full_workflow_secp256k1() {
    use gsigner::schemes::secp256k1::Secp256k1;

    let signer = Signer::<Secp256k1>::memory();

    // 1. Generate key
    let public1 = signer.generate().unwrap();

    // 2. Sign data
    let message = b"hello world";
    let sig1 = signer.sign(public1, message).unwrap();

    // 3. Verify signature
    signer.verify(public1, message, &sig1).unwrap();

    // 4. Export private key
    let private = signer.private_key(public1).unwrap();

    // 5. Create new signer and import key
    let signer2 = Signer::<Secp256k1>::memory();
    let public2 = signer2.import(private).unwrap();

    // 6. Public keys should match
    assert_eq!(public1, public2);

    // 7. Sign again with imported key
    let sig2 = signer2.sign(public2, message).unwrap();

    // 8. Both signatures should verify
    signer.verify(public1, message, &sig2).unwrap();
    signer2.verify(public2, message, &sig1).unwrap();
}

#[cfg(feature = "sr25519")]
#[test]
fn test_full_workflow_sr25519() {
    use gsigner::schemes::sr25519::Sr25519;

    let signer = Signer::<Sr25519>::memory();

    // Generate key
    let public1 = signer.generate().unwrap();

    // Sign data
    let message = b"hello sr25519";
    let sig1 = signer.sign(public1, message).unwrap();

    // Verify signature
    signer.verify(public1, message, &sig1).unwrap();

    // Export and reimport
    let private = signer.private_key(public1).unwrap();
    let signer2 = Signer::<Sr25519>::memory();
    let public2 = signer2.import(private).unwrap();

    assert_eq!(public1, public2);

    // Sign with reimported key
    let sig2 = signer2.sign(public2, message).unwrap();
    signer2.verify(public2, message, &sig2).unwrap();
}

#[cfg(feature = "ed25519")]
#[test]
fn test_full_workflow_ed25519() {
    use gsigner::schemes::ed25519::Ed25519;

    let signer = Signer::<Ed25519>::memory();

    // Generate key
    let public1 = signer.generate().unwrap();

    // Sign data
    let message = b"hello ed25519";
    let sig1 = signer.sign(public1, message).unwrap();

    // Verify signature
    signer.verify(public1, message, &sig1).unwrap();

    // Export and reimport
    let private = signer.private_key(public1).unwrap();
    let signer2 = Signer::<Ed25519>::memory();
    let public2 = signer2.import(private).unwrap();

    assert_eq!(public1, public2);

    // Sign with reimported key
    let sig2 = signer2.sign(public2, message).unwrap();
    signer2.verify(public2, message, &sig2).unwrap();
}

// =============================================================================
// Error handling tests
// =============================================================================

#[cfg(feature = "secp256k1")]
#[test]
fn test_sign_with_nonexistent_key() {
    use gsigner::schemes::secp256k1::{PrivateKey, Secp256k1};

    let signer = Signer::<Secp256k1>::memory();

    // Create a public key that doesn't exist in the signer
    let private = PrivateKey::random();
    let public = private.public_key();

    // Attempting to sign should fail
    let result = signer.sign(public, b"test");
    assert!(result.is_err());
}

#[cfg(feature = "secp256k1")]
#[test]
fn test_verify_with_wrong_signature() {
    use gsigner::schemes::secp256k1::Secp256k1;

    let signer = Signer::<Secp256k1>::memory();
    let public = signer.generate().unwrap();

    let message = b"correct message";
    let signature = signer.sign(public, message).unwrap();

    // Verify with wrong message should fail
    let wrong_message = b"wrong message";
    let result = signer.verify(public, wrong_message, &signature);
    assert!(result.is_err());
}

#[cfg(feature = "secp256k1")]
#[test]
fn test_wrong_password_error() {
    use gsigner::schemes::secp256k1::Secp256k1;

    let signer = Signer::<Secp256k1>::memory();

    // Generate key with password
    let public = signer.generate_encrypted("correct_password").unwrap();

    // Try to sign with wrong password
    let result = signer.sign_encrypted(public, b"test", "wrong_password");
    assert!(result.is_err());

    // Try to sign with no password (should also fail)
    let result = signer.sign(public, b"test");
    assert!(result.is_err());
}

// =============================================================================
// Edge case tests
// =============================================================================

#[cfg(feature = "secp256k1")]
#[test]
fn test_sign_empty_data() {
    use gsigner::schemes::secp256k1::Secp256k1;

    let signer = Signer::<Secp256k1>::memory();
    let public = signer.generate().unwrap();

    // Empty data should work
    let signature = signer.sign(public, b"").unwrap();
    signer.verify(public, b"", &signature).unwrap();
}

#[cfg(feature = "secp256k1")]
#[test]
fn test_sign_large_data() {
    use gsigner::schemes::secp256k1::Secp256k1;

    let signer = Signer::<Secp256k1>::memory();
    let public = signer.generate().unwrap();

    // Large data (1MB)
    let large_data = vec![0xABu8; 1024 * 1024];
    let signature = signer.sign(public, &large_data).unwrap();
    signer.verify(public, &large_data, &signature).unwrap();
}

#[cfg(feature = "secp256k1")]
#[test]
fn test_multiple_keys_in_signer() {
    use gsigner::schemes::secp256k1::Secp256k1;

    let signer = Signer::<Secp256k1>::memory();

    // Generate multiple keys
    let public1 = signer.generate().unwrap();
    let public2 = signer.generate().unwrap();
    let public3 = signer.generate().unwrap();

    assert_ne!(public1, public2);
    assert_ne!(public2, public3);
    assert_ne!(public1, public3);

    // All keys should be listed
    let keys = signer.list_keys().unwrap();
    assert_eq!(keys.len(), 3);
    assert!(keys.contains(&public1));
    assert!(keys.contains(&public2));
    assert!(keys.contains(&public3));

    // Each key should sign differently
    let message = b"test";
    let sig1 = signer.sign(public1, message).unwrap();
    let sig2 = signer.sign(public2, message).unwrap();
    let sig3 = signer.sign(public3, message).unwrap();

    // Signatures should be different
    assert_ne!(sig1, sig2);
    assert_ne!(sig2, sig3);
    assert_ne!(sig1, sig3);

    // Each signature should only verify with its corresponding key
    signer.verify(public1, message, &sig1).unwrap();
    assert!(signer.verify(public1, message, &sig2).is_err());
    assert!(signer.verify(public2, message, &sig1).is_err());
}

// =============================================================================
// Filesystem keyring tests
// =============================================================================

#[cfg(feature = "secp256k1")]
#[test]
fn test_filesystem_keyring_persistence() {
    use gsigner::schemes::secp256k1::Secp256k1;

    // Create a temporary directory for the keyring
    let temp_dir = tempfile::tempdir().unwrap();
    let keyring_path = temp_dir.path().to_path_buf();

    // Create signer and generate key
    let public = {
        let signer = Signer::<Secp256k1>::fs(keyring_path.clone()).unwrap();
        signer.generate().unwrap()
    };

    // Create new signer pointing to same path
    let signer2 = Signer::<Secp256k1>::fs(keyring_path).unwrap();

    // Key should still exist
    assert!(signer2.has_key(public).unwrap());

    // Should be able to sign with it
    let signature = signer2.sign(public, b"persisted").unwrap();
    signer2.verify(public, b"persisted", &signature).unwrap();
}

#[cfg(feature = "secp256k1")]
#[test]
fn test_sub_signer() {
    use gsigner::schemes::secp256k1::Secp256k1;

    let signer = Signer::<Secp256k1>::memory();

    // Generate multiple keys
    let public1 = signer.generate().unwrap();
    let public2 = signer.generate().unwrap();
    let public3 = signer.generate().unwrap();

    // Create sub-signer with only some keys
    let sub_signer = signer.sub_signer(vec![public1, public3]).unwrap();

    // Sub-signer should have the selected keys
    assert!(sub_signer.has_key(public1).unwrap());
    assert!(!sub_signer.has_key(public2).unwrap());
    assert!(sub_signer.has_key(public3).unwrap());

    // Sub-signer should be able to sign with its keys
    sub_signer.sign(public1, b"test").unwrap();
    sub_signer.sign(public3, b"test").unwrap();
    assert!(sub_signer.sign(public2, b"test").is_err());
}
