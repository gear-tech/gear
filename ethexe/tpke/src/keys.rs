// This file is part of Gear.
//
// Copyright (C) 2026 Gear Technologies Inc.
// SPDX-License-Identifier: GPL-3.0-or-later WITH Classpath-exception-2.0

use std::marker::PhantomData;

#[cfg(not(feature = "std"))]
use alloc::vec::Vec;

use ark_bls12_381::{Fr, G1Affine, G2Affine};
use parity_scale_codec::{Decode, Encode};
use scale_info::TypeInfo;
use zeroize::{Zeroize, ZeroizeOnDrop};

use crate::{
    Encryptable, TpkeError,
    bls12_381::{
        G1_COMPRESSED_LEN, G2_COMPRESSED_LEN, deserialize_compressed, serialize_g1, serialize_g2,
    },
};

/// Master secret key produced by the dealer. Must be destroyed after splitting.
///
/// The scalar field is private — callers can construct via [`Self::new`] and
/// read it via [`Self::scalar`], but cannot accidentally print or copy it
/// through direct field access. `Debug` is implemented to elide the scalar.
#[derive(Debug, Clone, Zeroize, ZeroizeOnDrop)]
pub struct MasterSecretKey(Fr);

impl MasterSecretKey {
    pub fn new(scalar: Fr) -> Self {
        Self(scalar)
    }

    pub fn scalar(&self) -> Fr {
        self.0
    }
}

/// Per-validator secret share `Sᵢ = f(i)`. Index is 1-based.
///
/// `scalar` is private; use [`Self::new`] to construct and [`Self::scalar`]
/// to read. The `index` is non-sensitive and stays public.
#[derive(Debug, Clone, Zeroize, ZeroizeOnDrop)]
pub struct SecretKeyShare {
    pub index: u32,
    scalar: Fr,
}

impl SecretKeyShare {
    pub fn new(index: u32, scalar: Fr) -> Self {
        Self { index, scalar }
    }

    pub fn scalar(&self) -> Fr {
        self.scalar
    }
}

/// Master public key `AggPub = S · g₂ ∈ G2`. Published openly.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct MasterPublicKey(pub G2Affine);

/// Per-validator share public key `PSᵢ = Sᵢ · g₂ ∈ G2`. Used by anyone to
/// verify a decryption share without knowing the secret.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct SharePublicKey {
    pub index: u32,
    pub point: G2Affine,
}

// TODO: move to another module

/// Ciphertext over encryptable object [Encryptable::EncryptedFields].
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Ciphertext<T> {
    inner: Vec<u8>,
    _data: PhantomData<fn() -> T>,
}

impl<T> Ciphertext<T> {
    pub fn new(inner: Vec<u8>) -> Ciphertext<T> {
        Self {
            inner,
            _data: PhantomData,
        }
    }
}

impl<T> AsRef<[u8]> for Ciphertext<T> {
    fn as_ref(&self) -> &[u8] {
        self.inner.as_ref()
    }
}

/// Encrypted ciphertext envelope. Wire format is the SCALE encoding of this.
#[derive(Clone, PartialEq, Eq, Debug, Encode, Decode, TypeInfo)]
pub struct Encrypted<T: Encryptable> {
    /// `U = u · g₂ ∈ G2`, compressed 96-byte serialization.
    pub u: [u8; G2_COMPRESSED_LEN],
    /// 32-byte identity binding (see `derive_id`).
    pub id: T::Id,
    /// ChaCha20-Poly1305 ciphertext incl. 16-byte Poly1305 tag.
    pub ciphertext: Ciphertext<T::EncryptedFields>,
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
pub struct DecryptionShare<T: Encryptable> {
    pub index: u32,
    pub id: T::Id,
    pub point: G1Affine,
}

/// Output of the dealer ceremony.
///
/// The `master_secret` is held in an `Option` and is accessible only via
/// [`take_master_secret`]. This makes the destruction step explicit: take it
/// once to persist or hand off, then let it drop (zeroized on drop). Cloning
/// `DealerOutput` clones the shares + pubs but does NOT clone the master
/// secret — subsequent clones see `None`. A leftover `master_secret` inside
/// `DealerOutput` is zeroized when the struct is dropped.
///
/// [`take_master_secret`]: DealerOutput::take_master_secret
#[derive(Debug)]
pub struct DealerOutput {
    pub(crate) master_secret: Option<MasterSecretKey>,
    pub master_pub: MasterPublicKey,
    pub shares: Vec<SecretKeyShare>,
    pub share_pubs: Vec<SharePublicKey>,
}

impl DealerOutput {
    /// Take ownership of the master secret. Returns `None` if it has already
    /// been taken or never existed. Subsequent calls return `None`.
    pub fn take_master_secret(&mut self) -> Option<MasterSecretKey> {
        self.master_secret.take()
    }
}

// SCALE codec for wire types. Manual impls are needed because arkworks' point
// types don't implement `Encode`/`Decode`/`TypeInfo`. The wire format uses
// BLS12-381 compressed encodings (48 B for G1, 96 B for G2). Encode panics
// only on serialization failure of an in-memory valid point, which cannot
// happen for points produced by this crate; Decode validates and returns a
// codec error on bad bytes.

// impl Encode for DecryptionShare {
//     fn encode_to<T: parity_scale_codec::Output + ?Sized>(&self, dest: &mut T) {
//         self.index.encode_to(dest);
//         self.id.encode_to(dest);
//         let bytes =
//             serialize_g1(&self.point).expect("DecryptionShare always holds a valid G1 point");
//         bytes.encode_to(dest);
//     }
// }

// impl Decode for DecryptionShare {
//     fn decode<I: parity_scale_codec::Input>(
//         input: &mut I,
//     ) -> Result<Self, parity_scale_codec::Error> {
//         let index = u32::decode(input)?;
//         let id = <[u8; 32]>::decode(input)?;
//         let bytes = <[u8; G1_COMPRESSED_LEN]>::decode(input)?;
//         let point = deserialize_compressed::<G1Affine, G1_COMPRESSED_LEN>(&bytes)
//             .map_err(|_| parity_scale_codec::Error::from("invalid G1 point in DecryptionShare"))?;
//         Ok(Self { index, id, point })
//     }
// }

impl Encode for MasterPublicKey {
    fn encode_to<T: parity_scale_codec::Output + ?Sized>(&self, dest: &mut T) {
        let bytes = serialize_g2(&self.0).expect("MasterPublicKey always holds a valid G2 point");
        bytes.encode_to(dest);
    }
}

impl Decode for MasterPublicKey {
    fn decode<I: parity_scale_codec::Input>(
        input: &mut I,
    ) -> Result<Self, parity_scale_codec::Error> {
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
            serialize_g2(&self.point).expect("SharePublicKey always holds a valid G2 point");
        bytes.encode_to(dest);
    }
}

impl Decode for SharePublicKey {
    fn decode<I: parity_scale_codec::Input>(
        input: &mut I,
    ) -> Result<Self, parity_scale_codec::Error> {
        let index = u32::decode(input)?;
        let bytes = <[u8; G2_COMPRESSED_LEN]>::decode(input)?;
        Self::from_bytes(index, &bytes).map_err(|_| {
            parity_scale_codec::Error::from("invalid or identity G2 point in SharePublicKey")
        })
    }
}

impl<T: Encryptable> DecryptionShare<T> {
    /// Serialize as `(index, id, compressed_point_bytes)`.
    pub fn to_bytes(&self) -> Result<(u32, &[u8], [u8; G1_COMPRESSED_LEN]), TpkeError> {
        Ok((self.index, self.id.as_ref(), serialize_g1(&self.point)?))
    }

    pub fn from_bytes(
        index: u32,
        id: T::Id,
        bytes: &[u8; G1_COMPRESSED_LEN],
    ) -> Result<Self, TpkeError> {
        let point = deserialize_compressed::<G1Affine, G1_COMPRESSED_LEN>(bytes)?;
        Ok(Self { index, id, point })
    }
}
