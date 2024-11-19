// This file is part of Gear.
//
// Copyright (C) 2024 Gear Technologies Inc.
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

use crate::{Digest, PrivateKey, PublicKey};
use anyhow::{Context, Error, Result};
use parity_scale_codec::{Decode, Encode};
use secp256k1::{
    ecdsa::{RecoverableSignature, RecoveryId},
    Message,
};
use std::fmt;

#[derive(Clone, Copy, PartialEq, Eq)]
pub struct RawSignature([u8; 65]);

impl RawSignature {
    pub fn create_for_digest(private_key: PrivateKey, digest: Digest) -> Result<RawSignature> {
        let secp_secret_key = secp256k1::SecretKey::from_slice(&private_key.0)
            .with_context(|| "Invalid secret key format for {:?}")?;

        let message = Message::from_digest(digest.into());

        let recoverable =
            secp256k1::global::SECP256K1.sign_ecdsa_recoverable(&message, &secp_secret_key);

        let (id, signature) = recoverable.serialize_compact();
        let mut bytes = [0u8; 65];
        bytes[..64].copy_from_slice(signature.as_ref());
        bytes[64] = i32::from(id) as u8;
        Ok(RawSignature(bytes))
    }
}

impl TryFrom<&[u8]> for RawSignature {
    type Error = Error;

    fn try_from(data: &[u8]) -> Result<Self> {
        let bytes = <[u8; 65]>::try_from(data)?;

        Ok(RawSignature(bytes))
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
        // TODO: Include chain id, as that's for transaction of pre-EIP-155 (!)
        sig.0[64] -= 27;
        RawSignature(sig.0)
    }
}

#[derive(Clone, Copy, Encode, Decode, PartialEq, Eq)]
pub struct Signature([u8; 65]);

impl Signature {
    pub fn create_for_digest(private_key: PrivateKey, digest: Digest) -> Result<Self> {
        let raw_signature = RawSignature::create_for_digest(private_key, digest)?;
        Ok(raw_signature.into())
    }

    pub fn to_hex(&self) -> String {
        hex::encode(self.0)
    }

    pub fn verify(&self, digest: Digest) -> Result<()> {
        let signature = (*self).try_into()?;
        let public_key = self.recover_from_digest_with_signature(Some(signature), digest)?;
        let secp256k1_pub_key = secp256k1::PublicKey::from_byte_array_compressed(&public_key.0)?;
        let message = Message::from_digest(digest.0);

        secp256k1::global::SECP256K1
            .verify_ecdsa(&message, &signature.to_standard(), &secp256k1_pub_key)
            .map_err(Into::into)
    }

    pub fn recover_from_digest(&self, digest: Digest) -> Result<PublicKey> {
        self.recover_from_digest_with_signature(None, digest)
    }

    fn recover_from_digest_with_signature(
        &self,
        signature: Option<RecoverableSignature>,
        digest: Digest,
    ) -> Result<PublicKey> {
        let signature = signature.unwrap_or((*self).try_into()?);
        signature
            .recover(&Message::from_digest(digest.0))
            .map(|pub_key| PublicKey::from_bytes(pub_key.serialize()))
            .map_err(Into::into)
    }
}

impl TryFrom<&[u8]> for Signature {
    type Error = Error;

    fn try_from(data: &[u8]) -> Result<Self> {
        let raw_signature = RawSignature::try_from(data)?;

        Ok(raw_signature.into())
    }
}

impl From<RawSignature> for Signature {
    fn from(mut sig: RawSignature) -> Self {
        // TODO: Include chain id, as that's for transaction of pre-EIP-155 (!)
        sig.0[64] += 27;
        Signature(sig.0)
    }
}

impl From<Signature> for [u8; 65] {
    fn from(sig: Signature) -> [u8; 65] {
        sig.0
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
