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

//! Secp256k1 signature types and utilities backed by `sp_core` primitives.

use super::{Address, Digest, PrivateKey, PublicKey};
use crate::{
    error::SignerError,
    hash::{Eip191Hash, keccak256_iter},
};
#[cfg(feature = "serde")]
use alloc::{format, string::String};
use core::hash::{Hash, Hasher};
use derive_more::{Debug, Display};
use k256::ecdsa::{self, RecoveryId};
#[cfg(feature = "codec")]
use parity_scale_codec::{
    Decode, Encode, Error as CodecError, Input as CodecInput, Output as CodecOutput,
};
#[cfg(feature = "serde")]
use serde::{Deserialize, Serialize};
use sha3::{Digest as _, Keccak256};
use sp_core::ecdsa::{Pair as SpPair, Public as SpPublic, Signature as SpSignature};

/// Result type used throughout signature helpers.
pub type SignResult<T> = Result<T, SignerError>;

type SignatureBytes = [u8; SIGNATURE_SIZE];
const SIGNATURE_SIZE: usize = 65;
const SIGNATURE_LAST_BYTE_IDX: usize = SIGNATURE_SIZE - 1;

/// A recoverable ECDSA signature.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Display)]
#[display("0x{}", hex::encode(self.into_pre_eip155_bytes()))]
pub struct Signature(SpSignature);

impl Signature {
    fn new(inner: SpSignature) -> Self {
        Self(normalize_signature(inner))
    }

    /// Create a recoverable signature for the provided data using the private key.
    pub fn create<T>(private_key: &PrivateKey, data: T) -> SignResult<Self>
    where
        Digest: From<T>,
    {
        let digest = Digest::from(data);
        Self::create_from_digest(private_key, digest)
    }

    /// Create a recoverable signature from a precomputed digest.
    pub fn create_from_digest(private_key: &PrivateKey, digest: Digest) -> SignResult<Self> {
        Ok(Self::new(private_key.as_pair().sign_prehashed(&digest.0)))
    }

    /// Create a recoverable signature from a precomputed digest.
    pub fn create_from_eip191_hash<T>(
        private_key: &PrivateKey,
        eip191_hash: Eip191Hash<T>,
    ) -> SignResult<Self> {
        Ok(Self::new(
            private_key.as_pair().sign_prehashed(eip191_hash.inner()),
        ))
    }

    /// Create a recoverable signature for the provided digest using the private key according to EIP-191.
    pub fn create_message<T>(private_key: &PrivateKey, data: T) -> SignResult<Self>
    where
        Digest: From<T>,
    {
        let digest = Digest::from(data);
        let eip191_hash = Self::eip191_hash(digest.0);

        Ok(Self::new(
            private_key.as_pair().sign_prehashed(&eip191_hash),
        ))
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
        self.0
            .recover_prehashed(&digest.0)
            .map(PublicKey::from)
            .ok_or_else(|| SignerError::Crypto("Failed to recover public key".into()))
    }

    pub fn recover_from_eip191_hash<T>(&self, hash: Eip191Hash<T>) -> SignResult<PublicKey> {
        self.0
            .recover_prehashed(hash.inner())
            .map(PublicKey::from)
            .ok_or_else(|| SignerError::Crypto("Failed to recover public key".into()))
    }

    /// Recovers public key which was used to create the signature for the signed message
    /// according to EIP-191 standard.
    pub fn recover_message<T>(&self, data: T) -> SignResult<PublicKey>
    where
        Digest: From<T>,
    {
        let eip191_hash = Self::eip191_hash(Digest::from(data).0);

        self.0
            .recover_prehashed(&eip191_hash)
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
        if SpPair::verify_prehashed(&self.0, &digest.0, &SpPublic::from(public_key)) {
            Ok(())
        } else {
            Err(SignerError::Crypto("Verification failed".into()))
        }
    }

    pub fn verify_with_eip191_hash<T>(
        &self,
        public_key: PublicKey,
        eip191_hash: Eip191Hash<T>,
    ) -> SignResult<()> {
        if SpPair::verify_prehashed(&self.0, eip191_hash.inner(), &SpPublic::from(public_key)) {
            Ok(())
        } else {
            Err(SignerError::Crypto("Verification failed".into()))
        }
    }

    /// Verifies message using [`Self::verify`] method according to EIP-191 standard.
    pub fn verify_message<T>(&self, public_key: PublicKey, data: T) -> SignResult<()>
    where
        Digest: From<T>,
    {
        let eip191_hash = Self::eip191_hash(Digest::from(data).0);

        if SpPair::verify_prehashed(&self.0, &eip191_hash, &SpPublic::from(public_key)) {
            Ok(())
        } else {
            Err(SignerError::Crypto("Verification failed".into()))
        }
    }

    fn eip191_hash(hash: [u8; 32]) -> [u8; 32] {
        let mut hasher = Keccak256::new();

        hasher.update(b"\x19Ethereum Signed Message:\n");
        hasher.update(b"32");
        hasher.update(hash.as_ref());

        hasher.finalize().into()
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

    /// Signature validation: verify the signature with public key recovery from the signature
    /// of the message signed according to EIP-191 standard.
    pub fn validate_message<T>(&self, data: T) -> SignResult<PublicKey>
    where
        Digest: From<T>,
    {
        let digest = Digest::from(data);
        let public_key = self.recover_message::<Digest>(digest)?;
        self.verify_message::<Digest>(public_key, digest)
            .map(|_| public_key)
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
        let mut bytes: SignatureBytes = self.0.into();
        bytes[SIGNATURE_LAST_BYTE_IDX] += 27;
        bytes
    }

    /// Returns internal signature bytes with raw recovery id.
    pub fn as_raw_bytes(&self) -> SignatureBytes {
        self.0.into()
    }

    /// Return the inner signature and recovery id as `k256` primitives.
    pub fn into_parts(self) -> (ecdsa::Signature, RecoveryId) {
        signature_and_recovery(self.0)
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

#[cfg(feature = "serde")]
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

#[cfg(feature = "serde")]
impl Serialize for Signature {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        let hex_string = format!("0x{}", hex::encode(self.into_pre_eip155_bytes()));
        hex_string.serialize(serializer)
    }
}

/// A signed data structure that contains the data and its signature.
#[derive(Clone, PartialEq, Eq, Debug, Display, Hash)]
#[cfg_attr(feature = "codec", derive(Encode))]
#[cfg_attr(feature = "serde", derive(Serialize))]
#[display("SignedData({data}, {signature})")]
pub struct SignedData<T: Sized> {
    data: T,
    signature: Signature,
    #[cfg_attr(feature = "codec", codec(skip))]
    #[cfg_attr(feature = "serde", serde(skip))]
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

#[cfg(feature = "serde")]
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

/// A signed according to EIP-191 message,that contains the data and its signature.
/// Always valid after construction.
#[derive(Clone, Encode, PartialEq, Eq, Debug, Display, Hash)]
#[cfg_attr(feature = "std", derive(serde::Serialize))]
#[display("SignedMessage({data}, {signature}, {address})")]
pub struct SignedMessage<T: Sized> {
    data: T,
    signature: Signature,
    address: Address,
}

impl<T: Sized> SignedMessage<T> {
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

    /// Returns the address of the signer.
    pub fn address(&self) -> Address {
        self.address
    }
}

impl<T: Sized> SignedMessage<T>
where
    for<'a> Digest: From<&'a T>,
{
    pub fn into_verified(self) -> VerifiedData<T> {
        let data = self.data;
        let public_key = self
            .signature
            .validate_message(&data)
            .expect("SignedMessage is always valid after construction");
        VerifiedData { data, public_key }
    }
}

impl<T: Sized + Decode> Decode for SignedMessage<T>
where
    for<'a> Digest: From<&'a T>,
{
    fn decode<I: CodecInput>(input: &mut I) -> Result<Self, CodecError> {
        let data = T::decode(input)?;
        let signature = Signature::decode(input)?;
        let address = Address::decode(input)?;

        Self::try_from_parts(data, signature, address).map_err(CodecError::from)
    }
}

#[cfg(feature = "std")]
impl<'de, T: Sized + serde::Deserialize<'de>> serde::Deserialize<'de> for SignedMessage<T>
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
            address: Address,
        }

        let Inner {
            data,
            signature,
            address,
        } = serde::Deserialize::deserialize(deserializer)?;

        Self::try_from_parts(data, signature, address).map_err(serde::de::Error::custom)
    }
}

impl<T: Sized> SignedMessage<T>
where
    for<'a> Digest: From<&'a T>,
{
    pub fn create(private_key: PrivateKey, data: T) -> SignResult<Self> {
        let signature = Signature::create_message(&private_key, &data)?;
        let public_key = PublicKey::from(&private_key);

        Ok(Self {
            data,
            signature,
            address: public_key.to_address(),
        })
    }

    pub fn try_from_parts(
        data: T,
        signature: Signature,
        address: Address,
    ) -> Result<Self, &'static str> {
        let pubkey = signature
            .validate_message(&data)
            .map_err(|_| "Invalid signature or attached data")?;

        if pubkey.to_address() != address {
            return Err("Address mismatch");
        }

        Ok(Self {
            data,
            signature,
            address,
        })
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
            contract_specific_digest(Digest::from(data), contract_address),
        )
        .map(ContractSignature)
    }

    pub fn create_from_digest(
        contract_address: Address,
        private_key: &PrivateKey,
        digest: Digest,
    ) -> SignResult<Self> {
        Signature::create_from_digest(
            private_key,
            contract_specific_digest(digest, contract_address),
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
    fn signature_recover_from_digest() {
        let private_key = mock_private_key();

        let signature = Signature::create(&private_key, MOCK_DIGEST).unwrap();
        let public_key = signature.recover(MOCK_DIGEST).unwrap();

        assert_eq!(PublicKey::from(private_key), public_key);
    }

    #[test]
    fn signed_data() {
        let private_key = mock_private_key();
        let public_key = private_key.public_key();
        let data = vec![1, 2, 3, 4];

        let signed_data = SignedData::create(&private_key, data.as_slice()).unwrap();
        assert_eq!(signed_data.public_key(), public_key);
        assert_eq!(signed_data.address(), public_key.to_address());
        assert_eq!(signed_data.data(), &data);
        assert_eq!(signed_data.signature().recover(&data).unwrap(), public_key);
        assert_eq!(signed_data.signature().validate(&data).unwrap(), public_key);
        signed_data.signature().verify(public_key, &data).unwrap();
    }

    #[test]
    fn contract_signature() {
        let private_key = mock_private_key();
        let address = private_key.public_key().to_address();

        let contract_signature =
            ContractSignature::create(CONTRACT_ADDRESS, &private_key, MOCK_DIGEST).unwrap();
        let public_key = contract_signature
            .validate(CONTRACT_ADDRESS, MOCK_DIGEST)
            .unwrap();
        assert_eq!(public_key.to_address(), address);
    }

    #[test]
    fn signature_encode_decode() {
        let private_key = mock_private_key();

        let signature = Signature::create(&private_key, MOCK_DIGEST).unwrap();
        let encoded = signature.encode();
        let decoded = Signature::decode(&mut &encoded[..]).unwrap();

        assert_eq!(signature, decoded);
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
    fn signature_recovery_matches_signing_key() {
        let private_key = mock_private_key();
        let expected_public = private_key.public_key();

        let signature = Signature::create(&private_key, MOCK_DIGEST).unwrap();
        let recovered = signature.validate(MOCK_DIGEST).unwrap();

        assert_eq!(expected_public, recovered);

        // Verify S is normalized (low)
        let (sig, _) = signature.into_parts();
        assert!(!bool::from(sig.s().is_high()));
    }

    #[cfg(feature = "std")]
    #[test]
    fn signed_message_roundtrip() {
        // Generate deterministic keypair to avoid relying on brittle fixtures.
        let private_key = PrivateKey::from_seed([1; 32]).expect("valid seed");
        let data = vec![1u8, 2, 3];

        let signed = SignedMessage::create(private_key.clone(), data.clone()).expect("sign");

        // Signature recovery matches the signer.
        let recovered = signed.signature().recover_message(data).expect("recover");
        assert_eq!(recovered.to_address(), signed.address());

        // Serde roundtrip keeps signature and address intact.
        let json = serde_json::to_string(&signed).expect("serialize");
        let deserialized: SignedMessage<Vec<u8>> =
            serde_json::from_str(&json).expect("deserialize");
        assert_eq!(signed, deserialized);
    }
}
