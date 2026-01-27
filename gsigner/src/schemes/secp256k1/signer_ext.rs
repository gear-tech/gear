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

//! Secp256k1-specific signer extensions.

use super::{
    Address, ContractSignature, Digest, PublicKey, Secp256k1, Signature, SignedData, SignedMessage,
};
use crate::{
    Signer,
    error::{Result, SignerError},
};

/// Extension trait for Secp256k1 signers.
pub trait Secp256k1SignerExt {
    /// Create a recoverable ECDSA signature.
    fn sign_recoverable(&self, public_key: PublicKey, data: &[u8]) -> Result<Signature>;

    /// Create a recoverable ECDSA signature using the provided password.
    fn sign_recoverable_with_password(
        &self,
        public_key: PublicKey,
        data: &[u8],
        password: Option<&str>,
    ) -> Result<Signature>;

    /// Create a recoverable ECDSA signature from a precomputed digest.
    fn sign_digest(&self, public_key: PublicKey, digest: Digest) -> Result<Signature>;

    /// Create a recoverable ECDSA signature from a precomputed digest using the provided password.
    fn sign_digest_with_password(
        &self,
        public_key: PublicKey,
        digest: Digest,
        password: Option<&str>,
    ) -> Result<Signature>;

    /// Create signed data (signature + data).
    fn signed_data<T>(&self, public_key: PublicKey, data: T) -> Result<SignedData<T>>
    where
        T: super::ToDigest;

    /// Create signed data (signature + data) using the provided password.
    fn signed_data_with_password<T>(
        &self,
        public_key: PublicKey,
        data: T,
        password: Option<&str>,
    ) -> Result<SignedData<T>>
    where
        T: super::ToDigest;

    /// Create a signed EIP-191 message with recovered address.
    fn signed_message<T>(&self, public_key: PublicKey, data: T) -> Result<SignedMessage<T>>
    where
        for<'a> Digest: From<&'a T>;

    /// Create a signed EIP-191 message with recovered address using the provided password.
    fn signed_message_with_password<T>(
        &self,
        public_key: PublicKey,
        data: T,
        password: Option<&str>,
    ) -> Result<SignedMessage<T>>
    where
        for<'a> Digest: From<&'a T>;

    /// Create a contract-specific signature (EIP-191).
    fn sign_for_contract(
        &self,
        contract_address: Address,
        public_key: PublicKey,
        data: &[u8],
    ) -> Result<ContractSignature>;

    /// Create a contract-specific signature (EIP-191) using the provided password.
    fn sign_for_contract_with_password(
        &self,
        contract_address: Address,
        public_key: PublicKey,
        data: &[u8],
        password: Option<&str>,
    ) -> Result<ContractSignature>;

    /// Create a contract-specific signature from a precomputed digest.
    fn sign_for_contract_digest(
        &self,
        contract_address: Address,
        public_key: PublicKey,
        digest: Digest,
    ) -> Result<ContractSignature>;

    /// Create a contract-specific signature from a precomputed digest using the provided password.
    fn sign_for_contract_digest_with_password(
        &self,
        contract_address: Address,
        public_key: PublicKey,
        digest: Digest,
        password: Option<&str>,
    ) -> Result<ContractSignature>;
}

impl Secp256k1SignerExt for Signer<Secp256k1> {
    fn sign_recoverable(&self, public_key: PublicKey, data: &[u8]) -> Result<Signature> {
        self.sign_recoverable_with_password(public_key, data, None)
    }

    fn sign_recoverable_with_password(
        &self,
        public_key: PublicKey,
        data: &[u8],
        password: Option<&str>,
    ) -> Result<Signature> {
        let private_key = self.get_private_key_with_password(public_key, password)?;
        Signature::create(&private_key, data)
            .map_err(|e| SignerError::Crypto(format!("Signature creation failed: {e}")))
    }

    fn sign_digest(&self, public_key: PublicKey, digest: Digest) -> Result<Signature> {
        self.sign_digest_with_password(public_key, digest, None)
    }

    fn sign_digest_with_password(
        &self,
        public_key: PublicKey,
        digest: Digest,
        password: Option<&str>,
    ) -> Result<Signature> {
        let private_key = self.get_private_key_with_password(public_key, password)?;
        Signature::create_from_digest(&private_key, digest)
            .map_err(|e| SignerError::Crypto(format!("Signature creation failed: {e}")))
    }

    fn signed_data<T>(&self, public_key: PublicKey, data: T) -> Result<SignedData<T>>
    where
        T: super::ToDigest,
    {
        self.signed_data_with_password(public_key, data, None)
    }

    fn signed_data_with_password<T>(
        &self,
        public_key: PublicKey,
        data: T,
        password: Option<&str>,
    ) -> Result<SignedData<T>>
    where
        T: super::ToDigest,
    {
        let private_key = self.get_private_key_with_password(public_key, password)?;
        SignedData::create(&private_key, data)
            .map_err(|e| SignerError::Crypto(format!("SignedData creation failed: {e}")))
    }

    fn signed_message<T>(&self, public_key: PublicKey, data: T) -> Result<SignedMessage<T>>
    where
        for<'a> Digest: From<&'a T>,
    {
        self.signed_message_with_password(public_key, data, None)
    }

    fn signed_message_with_password<T>(
        &self,
        public_key: PublicKey,
        data: T,
        password: Option<&str>,
    ) -> Result<SignedMessage<T>>
    where
        for<'a> Digest: From<&'a T>,
    {
        let private_key = self.get_private_key_with_password(public_key, password)?;
        SignedMessage::create(private_key, data)
            .map_err(|e| SignerError::Crypto(format!("SignedMessage creation failed: {e}")))
    }

    fn sign_for_contract(
        &self,
        contract_address: Address,
        public_key: PublicKey,
        data: &[u8],
    ) -> Result<ContractSignature> {
        self.sign_for_contract_with_password(contract_address, public_key, data, None)
    }

    fn sign_for_contract_with_password(
        &self,
        contract_address: Address,
        public_key: PublicKey,
        data: &[u8],
        password: Option<&str>,
    ) -> Result<ContractSignature> {
        let private_key = self.get_private_key_with_password(public_key, password)?;
        ContractSignature::create(contract_address, &private_key, data)
            .map_err(|e| SignerError::Crypto(format!("Contract signature creation failed: {e}")))
    }

    fn sign_for_contract_digest(
        &self,
        contract_address: Address,
        public_key: PublicKey,
        digest: Digest,
    ) -> Result<ContractSignature> {
        self.sign_for_contract_digest_with_password(contract_address, public_key, digest, None)
    }

    fn sign_for_contract_digest_with_password(
        &self,
        contract_address: Address,
        public_key: PublicKey,
        digest: Digest,
        password: Option<&str>,
    ) -> Result<ContractSignature> {
        let private_key = self.get_private_key_with_password(public_key, password)?;
        ContractSignature::create_from_digest(contract_address, &private_key, digest)
            .map_err(|e| SignerError::Crypto(format!("Contract signature creation failed: {e}")))
    }
}
