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

pub use k256::ecdsa::signature::Result as SignResult;
use sha3::{Digest as _, Keccak256};

use super::{
    address::Address,
    digest::{Digest, ToDigest},
    keys::{PrivateKey, PublicKey},
};
use core::hash::{Hash, Hasher};
use derive_more::{Debug, Display};
use k256::ecdsa::{self, RecoveryId, SigningKey, VerifyingKey, signature::hazmat::PrehashVerifier};
use parity_scale_codec::{
    Decode, Encode, Error as CodecError, Input as CodecInput, Output as CodecOutput,
};

/// A recoverable ECDSA signature
#[derive(Clone, Copy, PartialEq, Eq, derive_more::Debug, derive_more::Display)]
#[debug("0x{}", hex::encode(self.into_pre_eip155_bytes()))]
#[display("0x{}", hex::encode(self.into_pre_eip155_bytes()))]
pub struct Signature {
    inner: ecdsa::Signature,
    recovery_id: RecoveryId,
}

type SignatureBytes = [u8; 65];
const SIGNATURE_SIZE: usize = size_of::<SignatureBytes>();
const SIGNATURE_LAST_BYTE_IDX: usize = SIGNATURE_SIZE - 1;

impl Signature {
    /// Create a recoverable signature for the provided digest using the private key.
    pub fn create<T>(private_key: PrivateKey, data: T) -> SignResult<Self>
    where
        Digest: From<T>,
    {
        let digest = Digest::from(data);
        let signature = SigningKey::from(private_key)
            .sign_prehash_recoverable(digest.as_ref())
            .map(|(inner, recovery_id)| Self { inner, recovery_id })?;

        debug_assert!(signature.validate::<Digest>(digest).is_ok());

        Ok(signature)
    }

    /// Create a recoverable signature for the provided digest using the private key according to EIP-191.
    pub fn create_message<T>(private_key: PrivateKey, data: T) -> SignResult<Self>
    where
        Digest: From<T>,
    {
        let digest = Digest::from(data);
        let eip191_hash = Self::eip191_hash(digest.0);

        let signature = SigningKey::from(private_key)
            .sign_prehash_recoverable(&eip191_hash)
            .map(|(inner, recovery_id)| Self { inner, recovery_id })?;

        debug_assert!(signature.validate_message::<Digest>(digest).is_ok());

        Ok(signature)
    }

    /// Recovers public key which was used to create the signature for the signed data.
    pub fn recover<T>(&self, data: T) -> SignResult<PublicKey>
    where
        Digest: From<T>,
    {
        VerifyingKey::recover_from_prehash(
            Digest::from(data).as_ref(),
            &self.inner,
            self.recovery_id,
        )
        .map(Into::into)
    }

    /// Recovers public key which was used to create the signature for the signed message
    /// according to EIP-191 standard.
    pub fn recover_message<T>(&self, data: T) -> SignResult<PublicKey>
    where
        Digest: From<T>,
    {
        let eip191_hash = Self::eip191_hash(Digest::from(data).0);

        VerifyingKey::recover_from_prehash(&eip191_hash, &self.inner, self.recovery_id)
            .map(Into::into)
    }

    /// Verifies the signature using the public key and data possibly signed with
    /// the public key.
    pub fn verify<T>(&self, public_key: PublicKey, data: T) -> SignResult<()>
    where
        Digest: From<T>,
    {
        VerifyingKey::from(public_key).verify_prehash(Digest::from(data).as_ref(), &self.inner)
    }

    /// Verifies message using [`Self::verify`] method according to EIP-191 standard.
    pub fn verify_message<T>(&self, public_key: PublicKey, data: T) -> SignResult<()>
    where
        Digest: From<T>,
    {
        let eip191_hash = Self::eip191_hash(Digest::from(data).0);
        VerifyingKey::from(public_key).verify_prehash(&eip191_hash, &self.inner)
    }

    fn eip191_hash(hash: [u8; 32]) -> [u8; 32] {
        let mut hasher = Keccak256::new();

        hasher.update(b"\x19Ethereum Signed Message:\n");
        hasher.update(b"32");
        hasher.update(hash.as_ref());

        hasher.finalize().into()
    }

    /// Signature validation: verify the signature with public key recovery from the signature.
    pub fn validate<T>(&self, data: T) -> SignResult<PublicKey>
    where
        Digest: From<T>,
    {
        let digest = Digest::from(data);
        let public_key = self.recover::<Digest>(digest)?;
        self.verify::<Digest>(public_key, digest)
            .map(|_| public_key)
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

    /// Creates a signature from the bytes in the pre-EIP-155 format.
    /// See also: <https://shorturl.at/ckQ3y>
    pub fn from_pre_eip155_bytes(bytes: SignatureBytes) -> Option<Self> {
        let v = bytes[SIGNATURE_LAST_BYTE_IDX];

        let recovery_byte = v.checked_sub(27).and_then(|v| (v <= 1).then_some(v))?;

        Some(Self {
            inner: ecdsa::Signature::from_slice(&bytes[..SIGNATURE_LAST_BYTE_IDX]).ok()?,
            recovery_id: RecoveryId::from_byte(recovery_byte).expect("UNREACHABLE: v is 27 or 28"),
        })
    }

    pub fn into_pre_eip155_bytes(self) -> SignatureBytes {
        let mut bytes = [0u8; SIGNATURE_SIZE];

        bytes[..SIGNATURE_LAST_BYTE_IDX].copy_from_slice(self.inner.to_bytes().as_ref());

        let v = self.recovery_id.to_byte();
        assert!(v == 0 || v == 1, "Invalid v value: {v}");
        bytes[SIGNATURE_LAST_BYTE_IDX] = v + 27;

        bytes
    }

    pub fn into_parts(self) -> (ecdsa::Signature, RecoveryId) {
        (self.inner, self.recovery_id)
    }
}

impl Decode for Signature {
    fn decode<I: CodecInput>(input: &mut I) -> Result<Self, CodecError> {
        let bytes = <SignatureBytes>::decode(input)?;
        Self::from_pre_eip155_bytes(bytes).ok_or(CodecError::from("Invalid bytes"))
    }
}

impl Encode for Signature {
    fn encode_to<T: CodecOutput + ?Sized>(&self, dest: &mut T) {
        dest.write(self.into_pre_eip155_bytes().as_slice());
    }

    fn encoded_size(&self) -> usize {
        SIGNATURE_SIZE
    }
}

#[cfg(feature = "std")]
impl<'de> serde::Deserialize<'de> for Signature {
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
impl serde::Serialize for Signature {
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

/// A signed data structure, that contains the data and its signature.
/// Always valid after construction.
#[derive(Clone, Encode, PartialEq, Eq, Debug, Display, Hash)]
#[cfg_attr(feature = "std", derive(serde::Serialize))]
#[display("SignedData({data}, {signature})")]
pub struct SignedData<T: Sized> {
    data: T,
    signature: Signature,
    #[codec(skip)]
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
    pub fn create(private_key: PrivateKey, data: T) -> SignResult<Self> {
        let signature = Signature::create(private_key, &data)?;
        let public_key = PublicKey::from(private_key);

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

/// A signature verified data structure, that contains the data and public key.
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
        let signature = Signature::create_message(private_key, &data)?;
        let public_key = PublicKey::from(private_key);

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
/// See also `contract_specific_digest` and explanation here: <https://eips.ethereum.org/EIPS/eip-191>
#[derive(Debug, Clone, Copy, Encode, Decode, PartialEq, Eq, Hash)]
pub struct ContractSignature(Signature);

impl ContractSignature {
    /// Create a recoverable contract-specific signature for the provided data using the private key.
    pub fn create<T>(
        contract_address: Address,
        private_key: PrivateKey,
        data: T,
    ) -> SignResult<Self>
    where
        Digest: From<T>,
    {
        Signature::create::<Digest>(
            private_key,
            contract_specific_digest(Digest::from(data), contract_address),
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
    [
        [0x19, 0x00].as_ref(),
        contract_address.0.as_ref(),
        digest.as_ref(),
    ]
    .concat()
    .to_digest()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn mock_private_key() -> PrivateKey {
        PrivateKey::from([42; 32])
    }

    const MOCK_DIGEST: Digest = Digest([43; 32]);
    const CONTRACT_ADDRESS: Address = Address([44; 20]);

    #[test]
    fn signature_create_for_digest() {
        let private_key = mock_private_key();

        let signature = Signature::create(private_key, MOCK_DIGEST).unwrap();
        signature.validate(MOCK_DIGEST).unwrap();
    }

    #[test]
    fn signature_from_pre_eip155_bytes() {
        let private_key = mock_private_key();

        let signature = Signature::create(private_key, MOCK_DIGEST).unwrap();
        let bytes = signature.into_pre_eip155_bytes();

        let recovered_signature = Signature::from_pre_eip155_bytes(bytes).unwrap();
        assert_eq!(signature, recovered_signature);

        assert!(bytes[SIGNATURE_LAST_BYTE_IDX] == 27 || bytes[SIGNATURE_LAST_BYTE_IDX] == 28);
    }

    #[test]
    fn signature_validate() {
        let private_key = mock_private_key();

        Signature::create(private_key, MOCK_DIGEST)
            .unwrap()
            .validate(MOCK_DIGEST)
            .unwrap();
    }

    #[test]
    fn signature_recover_from_digest() {
        let private_key = mock_private_key();

        let signature = Signature::create(private_key, MOCK_DIGEST).unwrap();
        let public_key = signature.recover(MOCK_DIGEST).unwrap();

        assert_eq!(PublicKey::from(private_key), public_key);
    }

    #[test]
    fn signed_data() {
        let private_key = mock_private_key();
        let public_key = PublicKey::from(private_key);
        let data = vec![1, 2, 3, 4];

        let signed_data = SignedData::create(private_key, data.as_slice()).unwrap();
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
        let address = PublicKey::from(private_key).to_address();

        let contract_signature =
            ContractSignature::create(CONTRACT_ADDRESS, private_key, MOCK_DIGEST).unwrap();
        let public_key = contract_signature
            .validate(CONTRACT_ADDRESS, MOCK_DIGEST)
            .unwrap();
        assert_eq!(public_key.to_address(), address);
    }

    #[test]
    fn signature_encode_decode() {
        let private_key = mock_private_key();

        let signature = Signature::create(private_key, MOCK_DIGEST).unwrap();
        let encoded = signature.encode();
        let decoded = Signature::decode(&mut &encoded[..]).unwrap();

        assert_eq!(signature, decoded);
    }

    #[test]
    fn signed_data_encode_decode() {
        let private_key = mock_private_key();
        let data = vec![1, 2, 3, 4];

        let signed_data = SignedData::create(private_key, data).unwrap();
        let encoded = signed_data.encode();
        let decoded = SignedData::decode(&mut &encoded[..]).unwrap();

        assert_eq!(signed_data, decoded);
    }

    #[test]
    fn contract_signature_encode_decode() {
        let private_key = mock_private_key();

        let contract_signature =
            ContractSignature::create(CONTRACT_ADDRESS, private_key, MOCK_DIGEST).unwrap();
        let encoded = contract_signature.encode();
        let decoded = ContractSignature::decode(&mut &encoded[..]).unwrap();

        assert_eq!(contract_signature, decoded);
    }

    #[cfg(feature = "std")]
    #[test]
    fn signed_message_and_injected_transactions() {
        use crate::injected::InjectedTransaction;

        const RPC_INPUT: &str = "{
            \"data\":{
                \"destination\":\"0xede8c947f1ce1a5add6c26c2db01ad1dcd377c72\",
                \"payload\":\"0x\",
                \"value\":0,
                \"reference_block\":\"0xb03574ea84ef2acbdbc8c04f8afb73c9d59f2fbd3bf82f37dcb2aa390372b702\",
                \"salt\":\"0x6c6db263a31830e072ea7f083e6a818df3074119be6eee60601a5f2f668db508\"
            },
            \"signature\":\"0xfeffc4dfc0d5d49bd036b12a7ff5163132b5a40c93a5d369d0af1f925851ad1412fb33b7632c4dac9c8828d194fcaf417d5a2a2583ba23195c0080e8b6890c0a1c\",
            \"address\":\"0xf39Fd6e51aad88F6F4ce6aB8827279cffFb92266\"
        }";

        let signed_tx: SignedMessage<InjectedTransaction> =
            serde_json::from_str(RPC_INPUT).expect("failed to deserialize SignedMessage");

        // AKA tx_hash
        assert_eq!(
            hex::encode(signed_tx.data().to_message_id()),
            "867184f57aa63ceeb4066c061098317388bbacbea309ebd09a7fd228469460ee"
        );

        assert_eq!(
            hex::encode(signed_tx.address().0),
            "f39fd6e51aad88f6f4ce6ab8827279cfffb92266"
        );

        assert_eq!(
            signed_tx
                .signature()
                .recover_message(signed_tx.data())
                .expect("failed to recover message")
                .to_address(),
            signed_tx.address()
        );
    }
}
