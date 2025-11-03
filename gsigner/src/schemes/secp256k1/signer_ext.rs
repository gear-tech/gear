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

//! Secp256k1-specific signer extensions.

use super::{Address, ContractSignature, PublicKey, Secp256k1, Signature, SignedData};
use crate::{
    Signer,
    error::{Result, SignerError},
};

/// Extension trait for Secp256k1 signers.
pub trait Secp256k1SignerExt {
    /// Create a recoverable ECDSA signature.
    fn sign_recoverable(&self, public_key: PublicKey, data: &[u8]) -> Result<Signature>;

    /// Create a recoverable ECDSA signature from a precomputed digest.
    fn sign_digest(&self, public_key: PublicKey, digest: &super::Digest) -> Result<Signature>;

    /// Create signed data (signature + data).
    fn signed_data<T>(&self, public_key: PublicKey, data: T) -> Result<SignedData<T>>
    where
        T: super::ToDigest;

    /// Create a contract-specific signature (EIP-191).
    fn sign_for_contract(
        &self,
        contract_address: Address,
        public_key: PublicKey,
        data: &[u8],
    ) -> Result<ContractSignature>;

    /// Create a contract-specific signature from a precomputed digest.
    fn sign_for_contract_digest(
        &self,
        contract_address: Address,
        public_key: PublicKey,
        digest: &super::Digest,
    ) -> Result<ContractSignature>;
}

impl Secp256k1SignerExt for Signer<Secp256k1> {
    fn sign_recoverable(&self, public_key: PublicKey, data: &[u8]) -> Result<Signature> {
        let private_key = self.get_private_key(public_key)?;
        Signature::create(&private_key, data)
            .map_err(|e| SignerError::Crypto(format!("Signature creation failed: {e}")))
    }

    fn sign_digest(&self, public_key: PublicKey, digest: &super::Digest) -> Result<Signature> {
        let private_key = self.get_private_key(public_key)?;
        Signature::create_from_digest(&private_key, digest)
            .map_err(|e| SignerError::Crypto(format!("Signature creation failed: {e}")))
    }

    fn signed_data<T>(&self, public_key: PublicKey, data: T) -> Result<SignedData<T>>
    where
        T: super::ToDigest,
    {
        let private_key = self.get_private_key(public_key)?;
        SignedData::create(&private_key, data)
            .map_err(|e| SignerError::Crypto(format!("SignedData creation failed: {e}")))
    }

    fn sign_for_contract(
        &self,
        contract_address: Address,
        public_key: PublicKey,
        data: &[u8],
    ) -> Result<ContractSignature> {
        let private_key = self.get_private_key(public_key)?;
        ContractSignature::create(contract_address, &private_key, data)
            .map_err(|e| SignerError::Crypto(format!("Contract signature creation failed: {e}")))
    }

    fn sign_for_contract_digest(
        &self,
        contract_address: Address,
        public_key: PublicKey,
        digest: &super::Digest,
    ) -> Result<ContractSignature> {
        let private_key = self.get_private_key(public_key)?;
        ContractSignature::create_from_digest(contract_address, &private_key, digest)
            .map_err(|e| SignerError::Crypto(format!("Contract signature creation failed: {e}")))
    }
}
