// This file is part of Gear.
//
// Copyright (C) 2026 Gear Technologies Inc.
// SPDX-License-Identifier: GPL-3.0-or-later WITH Classpath-exception-2.0

#[cfg(not(feature = "std"))]
use alloc::vec::Vec;

use ark_bls12_381::{Bls12_381, Fr, G1Affine, G2Affine};
use ark_ec::{AffineRepr, CurveGroup, pairing::Pairing};
use parity_scale_codec::{Decode, Encode};
use scale_info::TypeInfo;
use zeroize::{Zeroize, ZeroizeOnDrop};

use core::marker::PhantomData;
use sha2::Digest;

use crate::{
    Result, TpkeError,
    utils::{self, G1_COMPRESSED_LEN, G2_COMPRESSED_LEN},
};

/// Master secret key produced by the dealer.
#[derive(Debug, Clone, Zeroize, ZeroizeOnDrop)]
pub struct MasterSecretKey(Fr);

impl MasterSecretKey {
    pub fn new(scalar: Fr) -> Self {
        Self(scalar)
    }

    /// Converts self to [MasterPublicKey].
    pub fn to_public(self) -> MasterPublicKey {
        MasterPublicKey((G2Affine::generator() * self.0).into_affine())
    }
}

/// Master public key `AggPub = S · g₂ ∈ G2`. Published openly.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct MasterPublicKey(pub G2Affine);

impl MasterPublicKey {
    pub fn to_bytes(&self) -> Result<[u8; G2_COMPRESSED_LEN]> {
        utils::serialize_g2(&self.0)
    }

    /// Deserialize the master pubkey, rejecting the G2 identity point. An
    /// identity-element master pubkey would make `e(Q_id, pk) = 1_GT`, letting
    /// anyone with the ciphertext derive the DEM key without shares.
    pub fn from_bytes(bytes: &[u8; G2_COMPRESSED_LEN]) -> Result<Self> {
        let point = utils::deserialize_g2(bytes)?;
        if point.is_zero() {
            return Err(TpkeError::IdentityPublicKey);
        }
        Ok(Self(point))
    }
}
/// Per-validator secret share `Sᵢ = f(i)`. Index is 1-based.
#[derive(Debug, Clone, Zeroize, ZeroizeOnDrop)]
pub struct SecretKeyShare {
    pub index: u32,
    scalar: Fr,
}

impl SecretKeyShare {
    pub fn new(index: u32, scalar: Fr) -> Self {
        Self { index, scalar }
    }

    pub fn to_public(&self) -> SharePublicKey {
        SharePublicKey {
            index: self.index,
            point: (G2Affine::generator() * self.scalar).into_affine(),
        }
    }

    /// Validator-side: produce `Dᵢ = Sᵢ · Q_id` for the ciphertext's id.
    pub fn decrypt_share<P>(&self, encrypted: &Encrypted<P>) -> Result<DecryptionShare<P>>
    where
        P: Encode + Decode,
    {
        if self.index == 0 {
            return Err(TpkeError::ZeroShareIndex);
        }

        let q = utils::hash_to_g1(encrypted.hash)?;
        let point = (q * self.scalar).into_affine();
        Ok(DecryptionShare {
            index: self.index,
            hash: encrypted.hash,
            point,
        })
    }
}

/// Per-validator share public key `PSᵢ = Sᵢ · g₂ ∈ G2`. Used by anyone to
/// verify a decryption share without knowing the secret.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct SharePublicKey {
    pub index: u32,
    pub point: G2Affine,
}

impl SharePublicKey {
    /// Verify a decryption share: e(Dᵢ, g₂) ?= e(Q_id, PSᵢ).
    ///
    /// Returns `Ok(false)` when the share's validator index or envelope id
    /// doesn't match what we're verifying against.
    pub fn verify<P>(&self, envelope: &Encrypted<P>, share: &DecryptionShare<P>) -> Result<bool>
    where
        P: Encode + Decode,
    {
        if share.index != self.index || share.hash != envelope.hash {
            return Ok(false);
        }
        let q = utils::hash_to_g1(envelope.hash)?;
        let g2 = G2Affine::generator();
        Ok(Bls12_381::pairing(share.point, g2) == Bls12_381::pairing(q, self.point))
    }

    pub fn to_bytes(&self) -> Result<(u32, [u8; G2_COMPRESSED_LEN])> {
        Ok((self.index, utils::serialize_g2(&self.point)?))
    }

    /// Deserialize a share pubkey, rejecting the G2 identity point. An identity
    /// share-pubkey would make share verification accept any honest share for
    /// that index regardless of the underlying secret-share scalar.
    pub fn from_bytes(index: u32, bytes: &[u8; G2_COMPRESSED_LEN]) -> Result<Self> {
        let point = utils::deserialize_g2(bytes)?;
        if point.is_zero() {
            return Err(TpkeError::IdentityPublicKey);
        }
        Ok(Self { index, point })
    }
}

/// Ciphtertext that was created using [chacha20poly1305::ChaCha20Poly1305] and [chacha20poly1305::aead::Aead::encrypt].
#[derive(
    Clone, Debug, PartialEq, Eq, Encode, Decode, TypeInfo, derive_more::AsRef, derive_more::AsMut,
)]
#[cfg_attr(feature = "std", derive(serde::Serialize, serde::Deserialize))]
pub struct Ciphertext<T> {
    #[as_ref]
    #[as_mut]
    inner: Vec<u8>,
    #[cfg_attr(feature = "std", serde(skip))]
    _type: PhantomData<fn() -> T>,
}

impl<T> Ciphertext<T> {
    pub fn new(inner: Vec<u8>) -> Ciphertext<T> {
        Self {
            inner,
            _type: PhantomData,
        }
    }
}

/// Encrypted ciphertext envelope. Wire format is the SCALE encoding of this.
#[derive(Clone, PartialEq, Eq, Debug, Encode, Decode, TypeInfo)]
#[cfg_attr(feature = "std", derive(serde::Serialize, serde::Deserialize))]
pub struct Encrypted<T> {
    /// `U = u · g₂ ∈ G2`, compressed 96-byte serialization.
    #[cfg_attr(feature = "std", serde(with = "serde_arrays"))]
    pub u: [u8; G2_COMPRESSED_LEN],
    /// Blake2 hash of encrypted data.
    pub hash: Blake2b256Hash<T>,
    /// ChaCha20-Poly1305 ciphertext.
    pub ciphertext: Ciphertext<T>,
}

/// Decryption share `Dᵢ = Sᵢ · Q_id ∈ G1`. Validator index is 1-based.
///
/// The `id` field binds the share to the envelope it was produced for. `verify`
/// and `combine` reject shares whose id doesn't match the target envelope —
/// this prevents accidental cross-envelope mixing from silently producing
/// garbage plaintext (which would otherwise only surface as a cryptic AEAD
/// failure downstream).
///
/// SCALE wire format: `index: u32` ‖ `id: [u8; 32]` ‖ `compressed_point: [u8; 48]`.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct DecryptionShare<P> {
    pub index: u32,
    pub hash: Blake2b256Hash<P>,
    pub point: G1Affine,
}

/// Output of the dealer ceremony.
#[derive(Debug)]
pub struct DealerOutput {
    pub master_pub: MasterPublicKey,
    pub secret_shares: Vec<SecretKeyShare>,
    pub public_shares: Vec<SharePublicKey>,
}

// SCALE codec for wire types. Manual impls are needed because arkworks' point
// types don't implement `Encode`/`Decode`/`TypeInfo`. The wire format uses
// BLS12-381 compressed encodings (48 B for G1, 96 B for G2). Encode panics
// only on serialization failure of an in-memory valid point, which cannot
// happen for points produced by this crate; Decode validates and returns a
// codec error on bad bytes.

impl<P> Encode for DecryptionShare<P> {
    fn encode_to<O: parity_scale_codec::Output + ?Sized>(&self, dest: &mut O) {
        self.index.encode_to(dest);
        self.hash.encode_to(dest);
        let bytes = utils::serialize_g1(&self.point)
            .expect("DecryptionShare always holds a valid G1 point");
        bytes.encode_to(dest);
    }
}

impl<P> Decode for DecryptionShare<P>
where
    P: Decode,
{
    fn decode<I: parity_scale_codec::Input>(
        input: &mut I,
    ) -> core::result::Result<Self, parity_scale_codec::Error> {
        let index = u32::decode(input)?;
        let hash = Blake2b256Hash::<P>::decode(input)?;
        let bytes = <[u8; G1_COMPRESSED_LEN]>::decode(input)?;
        let point = utils::deserialize_compressed::<G1Affine, G1_COMPRESSED_LEN>(&bytes)
            .map_err(|_| parity_scale_codec::Error::from("invalid G1 point in DecryptionShare"))?;
        Ok(Self { index, hash, point })
    }
}

impl Encode for MasterPublicKey {
    fn encode_to<T: parity_scale_codec::Output + ?Sized>(&self, dest: &mut T) {
        let bytes =
            utils::serialize_g2(&self.0).expect("MasterPublicKey always holds a valid G2 point");
        bytes.encode_to(dest);
    }
}

impl Decode for MasterPublicKey {
    fn decode<I: parity_scale_codec::Input>(
        input: &mut I,
    ) -> core::result::Result<Self, parity_scale_codec::Error> {
        let bytes = <[u8; G2_COMPRESSED_LEN]>::decode(input)?;
        // Use from_bytes so identity-point rejection is centralized.
        Self::from_bytes(&bytes).map_err(|_| {
            parity_scale_codec::Error::from("invalid or identity G2 point in MasterPublicKey")
        })
    }
}

impl Encode for SharePublicKey {
    fn encode_to<T: parity_scale_codec::Output + ?Sized>(&self, dest: &mut T) {
        self.index.encode_to(dest);
        let bytes =
            utils::serialize_g2(&self.point).expect("SharePublicKey always holds a valid G2 point");
        bytes.encode_to(dest);
    }
}

impl Decode for SharePublicKey {
    fn decode<I: parity_scale_codec::Input>(
        input: &mut I,
    ) -> core::result::Result<Self, parity_scale_codec::Error> {
        let index = u32::decode(input)?;
        let bytes = <[u8; G2_COMPRESSED_LEN]>::decode(input)?;
        Self::from_bytes(index, &bytes).map_err(|_| {
            parity_scale_codec::Error::from("invalid or identity G2 point in SharePublicKey")
        })
    }
}

impl<P> DecryptionShare<P> {
    /// Serialize as `(index, id, compressed_point_bytes)`.
    pub fn to_bytes(&self) -> Result<(u32, &[u8], [u8; G1_COMPRESSED_LEN])> {
        Ok((
            self.index,
            self.hash.as_ref(),
            utils::serialize_g1(&self.point)?,
        ))
    }

    pub fn from_bytes(
        index: u32,
        hash: Blake2b256Hash<P>,
        bytes: &[u8; G1_COMPRESSED_LEN],
    ) -> Result<Self> {
        let point = utils::deserialize_compressed::<G1Affine, G1_COMPRESSED_LEN>(bytes)?;
        Ok(Self { index, hash, point })
    }
}

#[derive(
    Debug, derive_more::PartialEq, derive_more::Eq, derive_more::AsRef, Encode, Decode, TypeInfo,
)]
#[cfg_attr(feature = "std", derive(serde::Serialize, serde::Deserialize))]
pub struct Blake2b256Hash<T> {
    #[as_ref([u8])]
    digest: [u8; 32],
    #[cfg_attr(feature = "std", serde(skip))]
    _type: PhantomData<fn() -> T>,
}

impl<T> Blake2b256Hash<T> {
    pub fn digest(&self) -> [u8; 32] {
        self.digest
    }
}

impl<T: Encode> From<&T> for Blake2b256Hash<T> {
    fn from(value: &T) -> Self {
        let mut hasher = blake2::Blake2s256::new();
        hasher.update(value.encode());

        Self {
            digest: hasher.finalize().into(),
            _type: PhantomData,
        }
    }
}

impl<T> Copy for Blake2b256Hash<T> {}

impl<T> Clone for Blake2b256Hash<T> {
    fn clone(&self) -> Self {
        *self
    }
}
