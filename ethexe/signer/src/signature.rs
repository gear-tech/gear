// This file is part of Gear.
//
// Copyright (C) 2024-2025 Gear Technologies Inc.
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

//! Secp256k1 signature types and utilities.

use crate::{Address, Digest, PrivateKey, PublicKey, ToDigest};
use anyhow::{Error, Result};
use parity_scale_codec::{Decode, Encode};
use secp256k1::{
    ecdsa::{RecoverableSignature, RecoveryId},
    Message,
};
use std::fmt;

/// A recoverable ECDSA signature with `v` value in an `Electrum` notation.
///
/// 'Electrum' notation signatures define `v` to be from the `{0; 1}` set.
#[derive(Clone, Copy, PartialEq, Eq)]
pub struct RawSignature([u8; 65]);

impl RawSignature {
    /// Create a recoverable signature for the provided digest using the private key.
    pub fn create_for_digest(private_key: PrivateKey, digest: Digest) -> Result<RawSignature> {
        let secp_secret_key = private_key.into();
        let message = Message::from_digest(digest.into());

        let recoverable =
            secp256k1::global::SECP256K1.sign_ecdsa_recoverable(&message, &secp_secret_key);
        let (id, signature) = recoverable.serialize_compact();

        let mut ret = [0u8; 65];
        ret[..64].copy_from_slice(signature.as_ref());
        ret[64] = i32::from(id)
            .try_into()
            .expect("recovery id is within u8 range");

        Ok(RawSignature(ret))
    }
}

impl From<RawSignature> for [u8; 65] {
    fn from(sig: RawSignature) -> [u8; 65] {
        sig.0
    }
}

impl AsRef<[u8]> for RawSignature {
    fn as_ref(&self) -> &[u8] {
        &self.0
    }
}

impl From<Signature> for RawSignature {
    fn from(mut sig: Signature) -> RawSignature {
        // TODO #4365: https://github.com/gear-tech/gear/issues/4365
        sig.0[64] -= 27;
        RawSignature(sig.0)
    }
}

/// A recoverable ECDSA signature type with any possible `v`.
///
/// The signature can be in 'Electrum' notation, pre- or post- EIP-155 notations.
#[derive(Clone, Copy, Encode, Decode, PartialEq, Eq)]
pub struct Signature([u8; 65]);

impl Signature {
    /// Create a recoverable signature for the provided digest using the private key.
    pub fn create_for_digest(private_key: PrivateKey, digest: Digest) -> Result<Self> {
        let raw_signature = RawSignature::create_for_digest(private_key, digest)?;
        Ok(raw_signature.into())
    }

    /// # Safety
    /// This function is unsafe because it does not check the validity of the input bytes.
    pub const unsafe fn from_bytes(bytes: [u8; 65]) -> Self {
        Self(bytes)
    }

    /// Covert signature to hex string.
    pub fn to_hex(&self) -> String {
        hex::encode(self.0)
    }

    /// Verify the signature with public key recovery from the signature.
    pub fn verify_with_public_key_recover(&self, digest: Digest) -> Result<()> {
        let public_key = self.recover_from_digest(digest)?;
        self.verify(public_key, digest)
    }

    /// Recovers public key which was used to create the signature for the signed digest.
    pub fn recover_from_digest(&self, digest: Digest) -> Result<PublicKey> {
        let signature: RecoverableSignature = (*self).try_into()?;
        signature
            .recover(&Message::from_digest(digest.0))
            .map(PublicKey::from)
            .map_err(Into::into)
    }

    /// Verifies the signature using the public key and digest possibly signed with
    /// the public key.
    pub fn verify(&self, public_key: PublicKey, digest: Digest) -> Result<()> {
        let signature: RecoverableSignature = (*self).try_into()?;
        let message = Message::from_digest(digest.0);
        let secp256k1_public_key = public_key.into();

        secp256k1::global::SECP256K1
            .verify_ecdsa(&message, &signature.to_standard(), &secp256k1_public_key)
            .map_err(Into::into)
    }

    pub fn verify_address(&self, address: Address, digest: Digest) -> Result<()> {
        let public_key = self.recover_from_digest(digest)?;
        if public_key.to_address() != address {
            anyhow::bail!("Verification failed: public key does not match the address");
        }
        self.verify(public_key, digest)
    }
}

impl From<RawSignature> for Signature {
    fn from(mut sig: RawSignature) -> Self {
        // TODO #4365: https://github.com/gear-tech/gear/issues/4365
        sig.0[64] += 27;
        Signature(sig.0)
    }
}

impl From<Signature> for [u8; 65] {
    fn from(sig: Signature) -> [u8; 65] {
        sig.0
    }
}

impl TryFrom<&[u8]> for Signature {
    type Error = Error;

    fn try_from(mut data: &[u8]) -> Result<Self> {
        Decode::decode(&mut data).map_err(Into::into)
    }
}

impl AsRef<[u8]> for Signature {
    fn as_ref(&self) -> &[u8] {
        &self.0
    }
}

impl fmt::Debug for Signature {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "0x{}", self.to_hex())
    }
}

impl fmt::Display for Signature {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{}", self.to_hex())
    }
}

impl TryFrom<Signature> for RecoverableSignature {
    type Error = anyhow::Error;

    fn try_from(sig: Signature) -> Result<Self> {
        RecoverableSignature::from_compact(
            sig.0[..64].as_ref(),
            // TODO: Include chain id, as that's for transaction of pre-EIP-155 (!)
            RecoveryId::try_from((sig.0[64] - 27) as i32)?,
        )
        .map_err(Into::into)
    }
}

#[derive(Clone, Debug, Encode, Decode, PartialEq, Eq)]
pub struct SignedData<T: Sized> {
    data: T,
    signature: Signature,
}

impl<T: ToDigest + Sized> SignedData<T> {
    pub fn new(data: T, signature: Signature) -> Self {
        Self { data, signature }
    }

    /// Verify the signature with public key recovery from the signature.
    pub fn verify_with_public_key_recover(&self) -> Result<()> {
        self.signature
            .verify_with_public_key_recover(self.data.to_digest())
    }

    /// Recovers public key which was used to create the signature.
    pub fn recover(&self) -> Result<PublicKey> {
        self.signature.recover_from_digest(self.data.to_digest())
    }

    /// Verifies that signed data is signed by the public key.
    pub fn verify(&self, public_key: PublicKey) -> Result<()> {
        self.signature.verify(public_key, self.data.to_digest())
    }

    /// Verifies that signed data is signed by the public key and the address matches the public key.
    pub fn verify_address(&self, address: Address) -> Result<()> {
        self.signature
            .verify_address(address, self.data.to_digest())
    }

    pub fn data(&self) -> &T {
        &self.data
    }

    pub fn signature(&self) -> &Signature {
        &self.signature
    }

    pub fn into_parts(self) -> (T, Signature) {
        (self.data, self.signature)
    }
}

// impl SignedData<Digest> {
//     pub fn new_from_digest(data: Digest, signature: Signature) -> Self {
//         Self { data, signature }
//     }

//     /// Verify the signature with public key recovery from the signature.
//     pub fn verify_with_public_key_recover(&self) -> Result<()> {
//         self.signature.verify_with_public_key_recover(self.data)
//     }

//     /// Recovers public key which was used to create the signature.
//     pub fn recover(&self) -> Result<PublicKey> {
//         self.signature.recover_from_digest(self.data)
//     }

//     /// Verifies that signed data is signed by the public key.
//     pub fn verify(&self, public_key: PublicKey) -> Result<()> {
//         self.signature.verify(public_key, self.data)
//     }

//     pub fn data(&self) -> &Digest {
//         &self.data
//     }

//     pub fn into_parts(self) -> (Digest, Signature) {
//         (self.data, self.signature)
//     }
// }
