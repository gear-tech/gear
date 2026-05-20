// Copyright (C) Gear Technologies Inc.
// SPDX-License-Identifier: GPL-3.0-or-later WITH Classpath-exception-2.0

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
