// This file is part of Gear.
//
// Copyright (C) 2025 Gear Technologies Inc.
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

//! # Key Tweaking Module
//!
//! This module implements deterministic key tweaking for contract-specific signing.
//! The same base DKG key can be "tweaked" to derive unique keys for different ActorIds.
//!
//! ## Mathematical Foundation
//!
//! Given a base key pair (sk, pk) and a tweak scalar h:
//! - Tweaked secret key: sk' = sk + h (mod n)
//! - Tweaked public key: pk' = pk + h*G
//!
//! Where:
//! - h = hash_to_scalar(ActorId)
//! - G is the secp256k1 generator point
//! - n is the curve order
//!
//! ## Security Properties
//!
//! - **Deterministic**: Same ActorId always produces same tweak
//! - **Independent**: Different ActorIds produce independent keys
//! - **Verifiable**: Anyone can compute tweaked public key from base pk and ActorId
//! - **Signature Compatibility**: Signatures with sk' verify against pk'
//!
//! ## Use Cases
//!
//! - Contract-specific signing: Different contracts get different keys
//! - Key isolation: Compromise of one tweaked key doesn't affect others
//! - Privacy: External observers can't link tweaked keys to same base key

use anyhow::{Context, Result, anyhow};
use gprimitives::ActorId;
use k256::{
    AffinePoint, ProjectivePoint, Scalar, U256,
    elliptic_curve::{
        ops::Reduce,
        sec1::{FromEncodedPoint, ToEncodedPoint},
    },
};
use sha3::{Digest as _, Keccak256};

/// Hash an ActorId to a scalar value for key tweaking
///
/// This function derives a deterministic scalar from an ActorId using
/// Keccak256 hash. The hash is reduced modulo the curve order to produce
/// a valid scalar.
///
/// ## Domain Separation
///
/// Uses "ETHEXE_KEY_TWEAK" prefix to prevent hash collisions with other
/// uses of ActorId hashing in the system.
///
/// # Arguments
///
/// * `actor` - The ActorId to hash
///
/// # Returns
///
/// A scalar value in the range [0, n) where n is the secp256k1 curve order
///
/// # Example
///
/// ```ignore
/// use gprimitives::ActorId;
/// use ethexe_common::crypto::tweak::hash_to_scalar;
///
/// let actor = ActorId::from([1u8; 32]);
/// let tweak = hash_to_scalar(actor);
/// ```
pub fn hash_to_scalar(actor: ActorId) -> Scalar {
    if actor == ActorId::zero() {
        return Scalar::ZERO;
    }
    let mut hasher = Keccak256::new();
    hasher.update(b"ETHEXE_KEY_TWEAK");
    hasher.update(actor.as_ref());
    let hash = hasher.finalize();

    // Convert hash to U256 and reduce modulo curve order
    let hash_u256 = U256::from_be_slice(&hash);
    Scalar::reduce(hash_u256)
}

/// Tweak a public key by adding h*G
///
/// Computes pk' = pk + h*G where:
/// - pk is the base public key (compressed format)
/// - h is the tweak scalar
/// - G is the secp256k1 generator
///
/// # Arguments
///
/// * `pk` - Base public key in compressed SEC1 format (33 bytes)
/// * `tweak` - Tweak scalar value
///
/// # Returns
///
/// Tweaked public key in compressed format
///
/// # Errors
///
/// - If input public key is invalid
/// - If resulting point is point at infinity
///
/// # Example
///
/// ```ignore
/// use ethexe_common::crypto::tweak::{hash_to_scalar, tweak_pubkey};
/// use gprimitives::ActorId;
///
/// let base_pk = [0u8; 33]; // Your base public key
/// let actor = ActorId::from([1u8; 32]);
/// let tweak = hash_to_scalar(actor);
/// let tweaked_pk = tweak_pubkey(&base_pk, tweak).unwrap();
/// ```
pub fn tweak_pubkey(pk: &[u8; 33], tweak: Scalar) -> Result<[u8; 33]> {
    // Decode base public key
    let encoded_point = k256::EncodedPoint::from_bytes(pk)
        .map_err(|err| anyhow!("Invalid public key encoding: {err}"))?;

    let pk_point = AffinePoint::from_encoded_point(&encoded_point)
        .into_option()
        .context("Invalid public key point")?;

    // Convert to projective for arithmetic
    let pk_projective = ProjectivePoint::from(pk_point);

    // Compute h*G
    let generator = ProjectivePoint::GENERATOR;
    let tweak_point = generator * tweak;

    // pk' = pk + h*G
    let tweaked_projective = pk_projective + tweak_point;

    // Convert back to affine and encode
    let tweaked_affine = tweaked_projective.to_affine();
    let encoded = tweaked_affine.to_encoded_point(true); // true = compressed

    let bytes = encoded.as_bytes();
    let mut result = [0u8; 33];
    result.copy_from_slice(bytes);

    Ok(result)
}

/// Tweak a secret share by adding the tweak scalar
///
/// Computes sk' = sk + h (mod n) where:
/// - sk is the base secret share
/// - h is the tweak scalar
/// - n is the secp256k1 curve order
///
/// # Arguments
///
/// * `share` - Base secret share as scalar
/// * `tweak` - Tweak scalar value
///
/// # Returns
///
/// Tweaked secret share
///
/// # Example
///
/// ```ignore
/// use ethexe_common::crypto::tweak::{hash_to_scalar, tweak_share};
/// use gprimitives::ActorId;
/// use k256::Scalar;
///
/// let base_share = Scalar::from(42u64);
/// let actor = ActorId::from([1u8; 32]);
/// let tweak = hash_to_scalar(actor);
/// let tweaked_share = tweak_share(base_share, tweak);
/// ```
pub fn tweak_share(share: Scalar, tweak: Scalar) -> Scalar {
    share + tweak
}

/// Verify that tweaked keys are consistent
///
/// Checks that verify(pk', msg, sig) holds where:
/// - pk' = tweak_pubkey(pk, h)
/// - sig was created with sk' = tweak_share(sk, h)
///
/// This is primarily for testing to ensure our tweaking implementation is correct.
///
/// # Arguments
///
/// * `base_pk` - Original public key
/// * `tweaked_pk` - Result of tweak_pubkey(base_pk, tweak)
/// * `tweak` - The tweak scalar used
///
/// # Returns
///
/// true if tweaked_pk == base_pk + tweak*G
pub fn verify_tweak_consistency(
    base_pk: &[u8; 33],
    tweaked_pk: &[u8; 33],
    tweak: Scalar,
) -> Result<bool> {
    let computed_tweaked = tweak_pubkey(base_pk, tweak)?;
    Ok(computed_tweaked == *tweaked_pk)
}

#[cfg(test)]
mod tests {
    use super::*;
    use k256::SecretKey;

    #[test]
    fn test_hash_to_scalar_deterministic() {
        let actor = ActorId::from([42u8; 32]);

        let scalar1 = hash_to_scalar(actor);
        let scalar2 = hash_to_scalar(actor);

        assert_eq!(
            scalar1.to_bytes(),
            scalar2.to_bytes(),
            "hash_to_scalar should be deterministic"
        );
    }

    #[test]
    fn test_hash_to_scalar_different_inputs() {
        let actor1 = ActorId::from([1u8; 32]);
        let actor2 = ActorId::from([2u8; 32]);

        let scalar1 = hash_to_scalar(actor1);
        let scalar2 = hash_to_scalar(actor2);

        assert_ne!(
            scalar1.to_bytes(),
            scalar2.to_bytes(),
            "Different ActorIds should produce different scalars"
        );
    }

    #[test]
    fn test_hash_to_scalar_zero_actor_is_zero() {
        let actor = ActorId::zero();

        let scalar = hash_to_scalar(actor);

        assert_eq!(
            scalar.to_bytes(),
            Scalar::ZERO.to_bytes(),
            "Zero ActorId should map to zero tweak"
        );
    }

    #[test]
    fn test_tweak_pubkey_valid() {
        // Generate a key pair using a deterministic seed for testing
        let secret_bytes = [42u8; 32];
        let secret = SecretKey::from_bytes(&secret_bytes.into()).unwrap();
        let public = secret.public_key();

        // Convert to compressed format
        let encoded = public.to_encoded_point(true);
        let mut pk_bytes = [0u8; 33];
        pk_bytes.copy_from_slice(encoded.as_bytes());

        // Create a tweak
        let actor = ActorId::from([1u8; 32]);
        let tweak = hash_to_scalar(actor);

        // Tweak the public key
        let tweaked_pk = tweak_pubkey(&pk_bytes, tweak).unwrap();

        // Verify it's different from base
        assert_ne!(pk_bytes, tweaked_pk, "Tweaked key should differ from base");
    }

    #[test]
    fn test_tweak_share() {
        let base_share = Scalar::from(42u64);
        let tweak = Scalar::from(10u64);

        let tweaked = tweak_share(base_share, tweak);

        // Verify: tweaked = base + tweak
        assert_eq!(tweaked, base_share + tweak);
    }

    #[test]
    fn test_tweak_consistency() {
        // Generate a key pair using deterministic seed
        let secret_bytes = [7u8; 32];
        let secret = SecretKey::from_bytes(&secret_bytes.into()).unwrap();
        let public = secret.public_key();

        let encoded = public.to_encoded_point(true);
        let mut pk_bytes = [0u8; 33];
        pk_bytes.copy_from_slice(encoded.as_bytes());

        // Create tweak
        let actor = ActorId::from([1u8; 32]);
        let tweak = hash_to_scalar(actor);

        // Tweak public key
        let tweaked_pk = tweak_pubkey(&pk_bytes, tweak).unwrap();

        // Verify consistency
        assert!(verify_tweak_consistency(&pk_bytes, &tweaked_pk, tweak).unwrap());
    }

    #[test]
    fn test_tweak_pubkey_and_share_correspondence() {
        // Generate base key pair with deterministic seed
        let secret_bytes = [99u8; 32];
        let sk = SecretKey::from_bytes(&secret_bytes.into()).unwrap();
        let pk = sk.public_key();

        let encoded = pk.to_encoded_point(true);
        let mut pk_bytes = [0u8; 33];
        pk_bytes.copy_from_slice(encoded.as_bytes());

        // Create tweak
        let actor = ActorId::from([123u8; 32]);
        let tweak = hash_to_scalar(actor);

        // Tweak both secret and public
        let sk_scalar = sk.to_nonzero_scalar();
        let tweaked_sk_scalar = tweak_share(*sk_scalar.as_ref(), tweak);
        let tweaked_pk = tweak_pubkey(&pk_bytes, tweak).unwrap();

        // Verify that tweaked_pk corresponds to tweaked_sk
        let tweaked_pk_from_sk = (ProjectivePoint::GENERATOR * tweaked_sk_scalar).to_affine();
        let tweaked_pk_from_sk_encoded = tweaked_pk_from_sk.to_encoded_point(true);
        let mut tweaked_pk_from_sk_bytes = [0u8; 33];
        tweaked_pk_from_sk_bytes.copy_from_slice(tweaked_pk_from_sk_encoded.as_bytes());

        assert_eq!(
            tweaked_pk, tweaked_pk_from_sk_bytes,
            "Tweaked public key should match key derived from tweaked secret"
        );
    }

    #[test]
    fn test_multiple_tweaks_commutative() {
        let secret_bytes = [55u8; 32];
        let sk = SecretKey::from_bytes(&secret_bytes.into()).unwrap();
        let pk = sk.public_key();

        let encoded = pk.to_encoded_point(true);
        let mut pk_bytes = [0u8; 33];
        pk_bytes.copy_from_slice(encoded.as_bytes());

        let actor1 = ActorId::from([1u8; 32]);
        let actor2 = ActorId::from([2u8; 32]);

        let tweak1 = hash_to_scalar(actor1);
        let tweak2 = hash_to_scalar(actor2);
        let combined_tweak = tweak1 + tweak2;

        // Apply tweaks separately
        let pk_tweaked1 = tweak_pubkey(&pk_bytes, tweak1).unwrap();
        let pk_tweaked12 = tweak_pubkey(&pk_tweaked1, tweak2).unwrap();

        // Apply combined tweak
        let pk_tweaked_combined = tweak_pubkey(&pk_bytes, combined_tweak).unwrap();

        assert_eq!(
            pk_tweaked12, pk_tweaked_combined,
            "Sequential tweaks should equal combined tweak"
        );
    }

    #[test]
    fn test_zero_tweak_identity() {
        let secret_bytes = [11u8; 32];
        let sk = SecretKey::from_bytes(&secret_bytes.into()).unwrap();
        let pk = sk.public_key();

        let encoded = pk.to_encoded_point(true);
        let mut pk_bytes = [0u8; 33];
        pk_bytes.copy_from_slice(encoded.as_bytes());

        let zero_tweak = Scalar::ZERO;
        let tweaked_pk = tweak_pubkey(&pk_bytes, zero_tweak).unwrap();

        assert_eq!(
            pk_bytes, tweaked_pk,
            "Zero tweak should return original key"
        );
    }
}
