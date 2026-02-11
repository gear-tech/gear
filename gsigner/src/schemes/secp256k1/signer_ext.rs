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
    Address, ContractSignature, Digest, PrivateKey, PublicKey, Secp256k1, Signature, SignedData,
    SignedMessage,
};
use crate::{
    Signer,
    error::{Result, SignerError},
};

/// Extension trait for Secp256k1 signers.
pub trait Secp256k1SignerExt {
    /// Get private key, optionally decrypting with password.
    fn get_private_key(&self, public_key: PublicKey, password: Option<&str>) -> Result<PrivateKey>;

    /// Create a recoverable ECDSA signature.
    fn sign_recoverable(
        &self,
        public_key: PublicKey,
        data: &[u8],
        password: Option<&str>,
    ) -> Result<Signature>;

    /// Create a recoverable ECDSA signature from a precomputed digest.
    fn sign_digest(
        &self,
        public_key: PublicKey,
        digest: Digest,
        password: Option<&str>,
    ) -> Result<Signature>;

    // fn sign_eip191_hash<T>(
    //     &self,
    //     public_key: PublicKey,
    //     eip191_hash: Eip191Hash<T>,
    //     password: Option<&str>,
    // ) -> Result<Signature>;

    /// Create signed data (signature + data).
    fn signed_data<T>(
        &self,
        public_key: PublicKey,
        data: T,
        password: Option<&str>,
    ) -> Result<SignedData<T>>
    where
        T: super::ToDigest;

    /// Create a signed EIP-191 message with recovered address.
    fn signed_message<T>(
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
        password: Option<&str>,
    ) -> Result<ContractSignature>;

    /// Create a contract-specific signature from a precomputed digest.
    fn sign_for_contract_digest(
        &self,
        contract_address: Address,
        public_key: PublicKey,
        digest: Digest,
        password: Option<&str>,
    ) -> Result<ContractSignature>;
}

impl Secp256k1SignerExt for Signer<Secp256k1> {
    fn get_private_key(&self, public_key: PublicKey, password: Option<&str>) -> Result<PrivateKey> {
        match password {
            Some(pwd) => self.private_key_encrypted(public_key, pwd),
            None => self.private_key(public_key),
        }
    }

    fn sign_recoverable(
        &self,
        public_key: PublicKey,
        data: &[u8],
        password: Option<&str>,
    ) -> Result<Signature> {
        let private_key = self.get_private_key(public_key, password)?;
        Signature::create(&private_key, data)
            .map_err(|e| SignerError::Crypto(format!("Signature creation failed: {e}")))
    }

    fn sign_digest(
        &self,
        public_key: PublicKey,
        digest: Digest,
        password: Option<&str>,
    ) -> Result<Signature> {
        let private_key = self.get_private_key(public_key, password)?;
        Signature::create_from_digest(&private_key, digest)
            .map_err(|e| SignerError::Crypto(format!("Signature creation failed: {e}")))
    }

    // fn sign_eip191_hash<T>(
    //     &self,
    //     public_key: PublicKey,
    //     eip191_hash: Eip191Hash<T>,
    //     password: Option<&str>,
    // ) -> Result<Signature> {
    //     let private_key = self.get_private_key(public_key, password)?;
    //     Signature::create_from_eip191_hash(&private_key, eip191_hash)
    //         .map_err(|e| SignerError::Crypto(format!("Signature creation failed: {e}")))
    // }

    fn signed_data<T>(
        &self,
        public_key: PublicKey,
        data: T,
        password: Option<&str>,
    ) -> Result<SignedData<T>>
    where
        T: super::ToDigest,
    {
        let private_key = self.get_private_key(public_key, password)?;
        SignedData::create(&private_key, data)
            .map_err(|e| SignerError::Crypto(format!("SignedData creation failed: {e}")))
    }

    fn signed_message<T>(
        &self,
        public_key: PublicKey,
        data: T,
        password: Option<&str>,
    ) -> Result<SignedMessage<T>>
    where
        for<'a> Digest: From<&'a T>,
    {
        let private_key = self.get_private_key(public_key, password)?;
        SignedMessage::create(private_key, data)
            .map_err(|e| SignerError::Crypto(format!("SignedMessage creation failed: {e}")))
    }

    fn sign_for_contract(
        &self,
        contract_address: Address,
        public_key: PublicKey,
        data: &[u8],
        password: Option<&str>,
    ) -> Result<ContractSignature> {
        let private_key = self.get_private_key(public_key, password)?;
        ContractSignature::create(contract_address, &private_key, data)
            .map_err(|e| SignerError::Crypto(format!("Contract signature creation failed: {e}")))
    }

    fn sign_for_contract_digest(
        &self,
        contract_address: Address,
        public_key: PublicKey,
        digest: Digest,
        password: Option<&str>,
    ) -> Result<ContractSignature> {
        let private_key = self.get_private_key(public_key, password)?;
        ContractSignature::create_from_digest(contract_address, &private_key, digest)
            .map_err(|e| SignerError::Crypto(format!("Contract signature creation failed: {e}")))
    }
}
