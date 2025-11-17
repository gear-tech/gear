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

//! Secp256k1 signature types and utilities backed by `sp_core` primitives.

use super::{Address, Digest, PrivateKey, PublicKey};
use crate::{error::SignerError, hash::keccak256_iter};
use core::hash::{Hash, Hasher};
use derive_more::{Debug, Display};
use k256::ecdsa::{self, RecoveryId};
#[cfg(feature = "codec")]
use parity_scale_codec::{
    Decode, Encode, Error as CodecError, Input as CodecInput, Output as CodecOutput,
};
#[cfg(feature = "std")]
use serde::{Deserialize, Serialize};
use sp_core::ecdsa::{Pair as SpPair, Public as SpPublic, Signature as SpSignature};

/// Result type used throughout signature helpers.
pub type SignResult<T> = Result<T, SignerError>;

type SignatureBytes = [u8; SIGNATURE_SIZE];
const SIGNATURE_SIZE: usize = 65;
const SIGNATURE_LAST_BYTE_IDX: usize = SIGNATURE_SIZE - 1;

/// A recoverable ECDSA signature.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Display)]
#[display("0x{}", hex::encode(self.into_pre_eip155_bytes()))]
pub struct Signature {
    inner: SpSignature,
}

impl Signature {
    fn new(inner: SpSignature) -> Self {
        Self {
            inner: normalize_signature(inner),
        }
    }

    /// Create a recoverable signature for the provided data using the private key.
    pub fn create<T>(private_key: &PrivateKey, data: T) -> SignResult<Self>
    where
        Digest: From<T>,
    {
        let digest = Digest::from(data);
        Self::create_from_digest(private_key, &digest)
    }

    /// Create a recoverable signature from a precomputed digest.
    pub fn create_from_digest(private_key: &PrivateKey, digest: &Digest) -> SignResult<Self> {
        Ok(Self::new(private_key.as_pair().sign_prehashed(&digest.0)))
    }

    /// Recovers the public key which was used to create the signature for the signed data.
    pub fn recover<T>(&self, data: T) -> SignResult<PublicKey>
    where
        Digest: From<T>,
    {
        let digest = Digest::from(data);
        self.recover_from_digest(&digest)
    }

    /// Recovers the public key using a precomputed digest.
    pub fn recover_from_digest(&self, digest: &Digest) -> SignResult<PublicKey> {
        self.inner
            .recover_prehashed(&digest.0)
            .map(PublicKey::from)
            .ok_or_else(|| SignerError::Crypto("Failed to recover public key".into()))
    }

    /// Verifies the signature using the public key and data.
    pub fn verify<T>(&self, public_key: PublicKey, data: T) -> SignResult<()>
    where
        Digest: From<T>,
    {
        let digest = Digest::from(data);
        self.verify_with_digest(public_key, &digest)
    }

    /// Verifies the signature against a precomputed digest.
    pub fn verify_with_digest(&self, public_key: PublicKey, digest: &Digest) -> SignResult<()> {
        if SpPair::verify_prehashed(&self.inner, &digest.0, &SpPublic::from(public_key)) {
            Ok(())
        } else {
            Err(SignerError::Crypto("Verification failed".into()))
        }
    }

    /// Signature validation with recovery.
    pub fn validate<T>(&self, data: T) -> SignResult<PublicKey>
    where
        Digest: From<T>,
    {
        let digest = Digest::from(data);
        let public_key = self.recover_from_digest(&digest)?;
        self.verify_with_digest(public_key, &digest)?;
        Ok(public_key)
    }

    /// Creates a signature from the bytes in the pre-EIP-155 format (V in {27, 28}).
    pub fn from_pre_eip155_bytes(bytes: SignatureBytes) -> Option<Self> {
        let recovery = bytes[SIGNATURE_LAST_BYTE_IDX].checked_sub(27)?;
        if recovery > 3 {
            return None;
        }

        let mut inner_bytes = bytes;
        inner_bytes[SIGNATURE_LAST_BYTE_IDX] = recovery;
        Some(Self::new(SpSignature::from_raw(inner_bytes)))
    }

    /// Convert signature into the pre-EIP-155 encoded bytes (V in {27, 28}).
    pub fn into_pre_eip155_bytes(self) -> SignatureBytes {
        let mut bytes: SignatureBytes = self.inner.into();
        bytes[SIGNATURE_LAST_BYTE_IDX] += 27;
        bytes
    }

    /// Returns internal signature bytes with raw recovery id.
    pub fn as_raw_bytes(&self) -> SignatureBytes {
        self.inner.into()
    }

    /// Return the inner signature and recovery id as `k256` primitives.
    pub fn into_parts(self) -> (ecdsa::Signature, RecoveryId) {
        signature_and_recovery(self.inner)
    }
}

impl Hash for Signature {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.into_pre_eip155_bytes().hash(state);
    }
}

#[cfg(feature = "codec")]
impl Decode for Signature {
    fn decode<I: CodecInput>(input: &mut I) -> Result<Self, CodecError> {
        let bytes = <SignatureBytes>::decode(input)?;
        Self::from_pre_eip155_bytes(bytes).ok_or_else(|| CodecError::from("Invalid bytes"))
    }
}

#[cfg(feature = "codec")]
impl Encode for Signature {
    fn encode_to<T: CodecOutput + ?Sized>(&self, dest: &mut T) {
        dest.write(self.into_pre_eip155_bytes().as_slice());
    }

    fn encoded_size(&self) -> usize {
        SIGNATURE_SIZE
    }
}

#[cfg(feature = "std")]
impl<'de> Deserialize<'de> for Signature {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let hex_string: String = serde::Deserialize::deserialize(deserializer)?;
        let hex_string = hex_string.strip_prefix("0x").unwrap_or(&hex_string);

        let bytes = hex::decode(hex_string)
            .map_err(|_err| serde::de::Error::custom("Invalid hex string"))?;

        let bytes: [u8; SIGNATURE_SIZE] = bytes
            .try_into()
            .map_err(|_err| serde::de::Error::custom("Invalid signature size"))?;
        Signature::from_pre_eip155_bytes(bytes)
            .ok_or_else(|| serde::de::Error::custom("Invalid bytes"))
    }
}

#[cfg(feature = "std")]
impl Serialize for Signature {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        let hex_string = format!("0x{}", hex::encode(self.into_pre_eip155_bytes()));
        hex_string.serialize(serializer)
    }
}

impl Hash for Signature {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.into_pre_eip155_bytes().hash(state);
    }
}

/// A signed data structure that contains the data and its signature.
#[derive(Clone, PartialEq, Eq, Debug, Display, Hash)]
#[cfg_attr(feature = "codec", derive(Encode))]
#[cfg_attr(feature = "std", derive(Serialize))]
#[display("SignedData({data}, {signature})")]
pub struct SignedData<T: Sized> {
    data: T,
    signature: Signature,
    #[cfg_attr(feature = "codec", codec(skip))]
    #[cfg_attr(feature = "std", serde(skip))]
    public_key: PublicKey,
}

impl<T: Sized> SignedData<T> {
    pub fn data(&self) -> &T {
        &self.data
    }

    pub fn signature(&self) -> &Signature {
        &self.signature
    }

    pub fn into_data(self) -> T {
        self.data
    }

    pub fn into_parts(self) -> (T, Signature) {
        (self.data, self.signature)
    }

    /// Returns the public key used to sign the data.
    pub fn public_key(&self) -> PublicKey {
        self.public_key
    }

    /// Returns the address of the public key used to sign the data.
    pub fn address(&self) -> Address {
        self.public_key.to_address()
    }

    pub fn into_verified(self) -> VerifiedData<T> {
        VerifiedData {
            data: self.data,
            public_key: self.public_key,
        }
    }
}

#[cfg(feature = "codec")]
impl<T: Sized + Decode> Decode for SignedData<T>
where
    for<'a> Digest: From<&'a T>,
{
    fn decode<I: CodecInput>(input: &mut I) -> Result<Self, CodecError> {
        let data = T::decode(input)?;
        let signature = Signature::decode(input)?;
        Self::try_from_parts(data, signature).map_err(CodecError::from)
    }
}

#[cfg(feature = "std")]
impl<'de, T: Sized + serde::Deserialize<'de>> serde::Deserialize<'de> for SignedData<T>
where
    for<'a> Digest: From<&'a T>,
{
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        #[derive(serde::Deserialize)]
        struct Inner<T> {
            data: T,
            signature: Signature,
        }

        let Inner { data, signature } = serde::Deserialize::deserialize(deserializer)?;
        Self::try_from_parts(data, signature).map_err(serde::de::Error::custom)
    }
}

impl<T: Sized> SignedData<T>
where
    for<'a> Digest: From<&'a T>,
{
    pub fn create(private_key: &PrivateKey, data: T) -> SignResult<Self> {
        let signature = Signature::create(private_key, &data)?;
        let public_key = private_key.public_key();

        Ok(Self {
            data,
            signature,
            public_key,
        })
    }

    pub fn try_from_parts(data: T, signature: Signature) -> Result<Self, &'static str> {
        signature
            .validate(&data)
            .map_err(|_| "Invalid signature or attached data")
            .map(|public_key| Self {
                data,
                signature,
                public_key,
            })
    }
}

/// A signature verified data structure with the data and recovered public key.
#[derive(Clone, PartialEq, Eq, Debug, Display, Hash)]
#[display("ValidatedData({data}, {public_key})")]
pub struct VerifiedData<T> {
    data: T,
    public_key: PublicKey,
}

impl<T> VerifiedData<T> {
    pub fn map<U>(self, f: impl FnOnce(T) -> U) -> VerifiedData<U> {
        let Self { data, public_key } = self;
        let data = f(data);
        VerifiedData { data, public_key }
    }

    pub fn data(&self) -> &T {
        &self.data
    }

    pub fn into_parts(self) -> (T, PublicKey) {
        (self.data, self.public_key)
    }

    /// Returns the public key used to sign the data.
    pub fn public_key(&self) -> PublicKey {
        self.public_key
    }

    /// Returns the address of the public key used to sign the data.
    pub fn address(&self) -> Address {
        self.public_key.to_address()
    }
}

/// A recoverable ECDSA signature for a contract-specific digest format (ERC-191).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "codec", derive(Encode, Decode))]
pub struct ContractSignature(Signature);

impl ContractSignature {
    /// Create a ContractSignature from a Signature.
    pub fn from_signature(signature: Signature) -> Self {
        Self(signature)
    }

    /// Create a recoverable contract-specific signature for the provided data using the private key.
    pub fn create<T>(
        contract_address: Address,
        private_key: &PrivateKey,
        data: T,
    ) -> SignResult<Self>
    where
        Digest: From<T>,
    {
        Signature::create_from_digest(
            private_key,
            &contract_specific_digest(Digest::from(data), contract_address),
        )
        .map(ContractSignature)
    }

    pub fn create_from_digest(
        contract_address: Address,
        private_key: &PrivateKey,
        digest: &Digest,
    ) -> SignResult<Self> {
        Signature::create_from_digest(
            private_key,
            &contract_specific_digest(*digest, contract_address),
        )
        .map(ContractSignature)
    }

    pub fn validate<T>(&self, contract_address: Address, data: T) -> SignResult<PublicKey>
    where
        Digest: From<T>,
    {
        self.0.validate::<Digest>(contract_specific_digest(
            Digest::from(data),
            contract_address,
        ))
    }

    pub fn into_pre_eip155_bytes(self) -> [u8; 65] {
        self.0.into_pre_eip155_bytes()
    }
}

fn contract_specific_digest(digest: Digest, contract_address: Address) -> Digest {
    Digest(keccak256_iter([
        &[0x19, 0x00],
        contract_address.0.as_ref(),
        digest.as_ref(),
    ]))
}

fn signature_and_recovery(signature: SpSignature) -> (ecdsa::Signature, RecoveryId) {
    let bytes: SignatureBytes = signature.into();
    let recovery = RecoveryId::from_byte(bytes[SIGNATURE_LAST_BYTE_IDX])
        .expect("recovery id stored in signature is always <4");
    let sig = ecdsa::Signature::from_bytes((&bytes[..SIGNATURE_LAST_BYTE_IDX]).into())
        .expect("signature bytes are always valid");
    (sig, recovery)
}

fn normalize_signature(signature: SpSignature) -> SpSignature {
    let (mut sig, mut recovery) = signature_and_recovery(signature);

    if let Some(normalized) = sig.normalize_s() {
        let parity = !recovery.is_y_odd();
        recovery = RecoveryId::new(parity, recovery.is_x_reduced());
        sig = normalized;
    }

    let mut bytes = [0u8; SIGNATURE_SIZE];
    bytes[..SIGNATURE_LAST_BYTE_IDX].copy_from_slice(sig.to_bytes().as_slice());
    bytes[SIGNATURE_LAST_BYTE_IDX] = recovery.to_byte();
    SpSignature::from_raw(bytes)
}

#[cfg(test)]
mod tests {
    use super::*;
    use k256::elliptic_curve::scalar::IsHigh;

    fn mock_private_key() -> PrivateKey {
        PrivateKey::from_seed([42; 32]).expect("seed is valid")
    }

    const MOCK_DIGEST: Digest = Digest([43; 32]);
    const CONTRACT_ADDRESS: Address = Address([44; 20]);

    #[test]
    fn signature_create_for_digest() {
        let private_key = mock_private_key();

        let signature = Signature::create(&private_key, MOCK_DIGEST).unwrap();
        signature.validate(MOCK_DIGEST).unwrap();
        let (sig, _) = signature.into_parts();
        assert!(!bool::from(sig.s().is_high()));
    }

    #[test]
    fn signature_from_pre_eip155_bytes() {
        let private_key = mock_private_key();

        let signature = Signature::create(&private_key, MOCK_DIGEST).unwrap();
        let bytes = signature.into_pre_eip155_bytes();

        let recovered_signature = Signature::from_pre_eip155_bytes(bytes).unwrap();
        assert_eq!(signature, recovered_signature);

        assert!(bytes[SIGNATURE_LAST_BYTE_IDX] == 27 || bytes[SIGNATURE_LAST_BYTE_IDX] == 28);
    }

    #[test]
    fn signature_validate() {
        let private_key = mock_private_key();

        Signature::create(&private_key, MOCK_DIGEST)
            .unwrap()
            .validate(MOCK_DIGEST)
            .unwrap();
    }

    #[test]
    fn contract_signature_roundtrip() {
        let private_key = mock_private_key();

        let signature =
            ContractSignature::create(CONTRACT_ADDRESS, &private_key, MOCK_DIGEST).unwrap();

        signature.validate(CONTRACT_ADDRESS, MOCK_DIGEST).unwrap();
    }
}
