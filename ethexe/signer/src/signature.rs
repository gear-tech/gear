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
use anyhow::Result;
use derive_more::{Debug, Display};
use k256::ecdsa::{self, signature::hazmat::PrehashVerifier, RecoveryId, SigningKey, VerifyingKey};
use parity_scale_codec::{
    Decode, Encode, Error as CodecError, Input as CodecInput, Output as CodecOutput,
};

/// A recoverable ECDSA signature
#[derive(Clone, Copy, PartialEq, Eq, Debug, Display)]
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
    pub fn create<T>(private_key: PrivateKey, data: T) -> Result<Self>
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

    /// Recovers public key which was used to create the signature for the signed data.
    pub fn recover<T>(&self, data: T) -> Result<PublicKey>
    where
        Digest: From<T>,
    {
        VerifyingKey::recover_from_prehash(
            Digest::from(data).as_ref(),
            &self.inner,
            self.recovery_id,
        )
        .map_err(Into::into)
        .map(Into::into)
    }

    /// Verifies the signature using the public key and data possibly signed with
    /// the public key.
    pub fn verify<T>(&self, public_key: PublicKey, data: T) -> Result<()>
    where
        Digest: From<T>,
    {
        VerifyingKey::from(public_key)
            .verify_prehash(Digest::from(data).as_ref(), &self.inner)
            .map_err(Into::into)
    }

    /// Signature validation: verify the signature with public key recovery from the signature.
    pub fn validate<T>(&self, data: T) -> Result<PublicKey>
    where
        Digest: From<T>,
    {
        let digest = Digest::from(data);
        let public_key = self.recover::<Digest>(digest)?;
        self.verify::<Digest>(public_key, digest)
            .map(|_| public_key)
    }

    /// Creates a signature from the bytes in the pre-EIP-155 format.
    /// See also: https://shorturl.at/ckQ3y
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

/// A signed data structure, that contains the data and its signature.
/// Always valid after construction.
#[derive(Clone, Debug, Encode, PartialEq, Eq)]
pub struct SignedData<T: Sized> {
    data: T,
    signature: Signature,
    #[codec(skip)]
    public_key: PublicKey,
}

impl<T: Sized> SignedData<T> {
    pub fn data(&self) -> &T {
        &self.data
    }

    pub fn signature(&self) -> &Signature {
        &self.signature
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
}

impl<T: Sized + Decode> Decode for SignedData<T>
where
    for<'a> Digest: From<&'a T>,
{
    fn decode<I: CodecInput>(input: &mut I) -> Result<Self, CodecError> {
        let data = T::decode(input)?;
        let signature = Signature::decode(input)?;

        let public_key = signature
            .validate(&data)
            .map_err(|_| CodecError::from("Invalid signature or attached data"))?;

        Ok(Self {
            data,
            signature,
            public_key,
        })
    }
}

impl<T: Sized> SignedData<T>
where
    for<'a> Digest: From<&'a T>,
{
    pub fn create(private_key: PrivateKey, data: T) -> Result<Self> {
        let signature = Signature::create(private_key, &data)?;
        let public_key = PublicKey::from(private_key);

        Ok(Self {
            data,
            signature,
            public_key,
        })
    }
}

/// A recoverable ECDSA signature for a contract-specific digest format (ERC-191).
/// See also [`contract_specific_digest`] and explanation here: https://eips.ethereum.org/EIPS/eip-191
#[derive(Debug, Clone, Encode, Decode, PartialEq, Eq)]
pub struct ContractSignature(Signature);

impl ContractSignature {
    /// Create a recoverable contract-specific signature for the provided data using the private key.
    pub fn create<T>(contract_address: Address, private_key: PrivateKey, data: T) -> Result<Self>
    where
        Digest: From<T>,
    {
        Signature::create::<Digest>(
            private_key,
            contract_specific_digest(Digest::from(data), contract_address),
        )
        .map(ContractSignature)
    }

    pub fn validate<T>(&self, contract_address: Address, data: T) -> Result<PublicKey>
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

    fn mock_digest() -> Digest {
        Digest::from([43; 32])
    }

    const CONTRACT_ADDRESS: Address = Address([44; 20]);

    #[test]
    fn signature_create_for_digest() {
        let private_key = mock_private_key();
        let digest = mock_digest();

        let signature = Signature::create(private_key, digest).unwrap();
        signature.validate(digest).unwrap();
    }

    #[test]
    fn signature_from_pre_eip155_bytes() {
        let private_key = mock_private_key();
        let digest = mock_digest();

        let signature = Signature::create(private_key, digest).unwrap();
        let bytes = signature.into_pre_eip155_bytes();

        let recovered_signature = Signature::from_pre_eip155_bytes(bytes).unwrap();
        assert_eq!(signature, recovered_signature);

        assert!(bytes[SIGNATURE_LAST_BYTE_IDX] == 27 || bytes[SIGNATURE_LAST_BYTE_IDX] == 28);
    }

    #[test]
    fn signature_validate() {
        let private_key = mock_private_key();
        let digest = mock_digest();

        Signature::create(private_key, digest)
            .unwrap()
            .validate(digest)
            .unwrap();
    }

    #[test]
    fn signature_recover_from_digest() {
        let private_key = mock_private_key();
        let digest = mock_digest();

        let signature = Signature::create(private_key, digest).unwrap();
        let public_key = signature.recover(digest).unwrap();

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
        assert_eq!(
            signed_data.signature().recover(data.as_slice()).unwrap(),
            public_key
        );
        assert_eq!(
            signed_data.signature().validate(data.as_slice()).unwrap(),
            public_key
        );
        signed_data
            .signature()
            .verify(public_key, data.as_slice())
            .unwrap();
    }

    #[test]
    fn contract_signature() {
        let private_key = mock_private_key();
        let address = PublicKey::from(private_key).to_address();
        let digest = mock_digest();

        let contract_signature =
            ContractSignature::create(CONTRACT_ADDRESS, private_key, digest).unwrap();
        let public_key = contract_signature
            .validate(CONTRACT_ADDRESS, digest)
            .unwrap();
        assert_eq!(public_key.to_address(), address);
    }

    #[test]
    fn signature_encode_decode() {
        let private_key = mock_private_key();
        let digest = mock_digest();

        let signature = Signature::create(private_key, digest).unwrap();
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
        let digest = mock_digest();

        let contract_signature =
            ContractSignature::create(CONTRACT_ADDRESS, private_key, digest).unwrap();
        let encoded = contract_signature.encode();
        let decoded = ContractSignature::decode(&mut &encoded[..]).unwrap();

        assert_eq!(contract_signature, decoded);
    }
}
