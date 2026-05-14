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

//! Sr25519-specific signer extensions.

use super::{PrivateKey, PublicKey, Signature, Sr25519};
use crate::{Signer, scheme::CryptoScheme};
use anyhow::Result;
use schnorrkel::signing_context;

/// Extension trait for Sr25519 signers.
pub trait Sr25519SignerExt {
    /// Sign with a custom context. Pass `password: None` for unencrypted keys.
    fn sign_with_context(
        &self,
        public_key: PublicKey,
        context: &[u8],
        data: &[u8],
        password: Option<&str>,
    ) -> Result<Signature>;

    /// Verify a signature with the given context.
    fn verify_with_context(
        &self,
        public_key: PublicKey,
        context: &[u8],
        data: &[u8],
        signature: &Signature,
    ) -> Result<()>;

    /// Generate a vanity key. Pass `password: None` for unencrypted storage.
    fn generate_vanity(&self, prefix: &str, password: Option<&str>) -> Result<PublicKey>;
}

impl Sr25519SignerExt for Signer<Sr25519> {
    fn sign_with_context(
        &self,
        public_key: PublicKey,
        context: &[u8],
        data: &[u8],
        password: Option<&str>,
    ) -> Result<Signature> {
        let private_key = match password {
            Some(pwd) => self.private_key_encrypted(public_key, pwd)?,
            None => self.private_key(public_key)?,
        };
        let ctx = signing_context(context);
        let keypair = private_key.keypair();
        Ok(Signature::from(keypair.sign(ctx.bytes(data))))
    }

    fn verify_with_context(
        &self,
        public_key: PublicKey,
        context: &[u8],
        data: &[u8],
        signature: &Signature,
    ) -> Result<()> {
        let ctx = signing_context(context);
        let pub_key = public_key.to_schnorrkel()?;
        let sig = signature.to_schnorrkel()?;
        pub_key
            .verify(ctx.bytes(data), &sig)
            .map_err(|e| anyhow::anyhow!("Verification failed: {:?}", e))
    }

    fn generate_vanity(&self, prefix: &str, password: Option<&str>) -> Result<PublicKey> {
        use crate::address::{SubstrateAddress, SubstrateCryptoScheme};

        let mut attempts: u64 = 0;
        loop {
            attempts += 1;
            let candidate = PrivateKey::random();
            let public_key = Sr25519::public_key(&candidate);
            let address =
                SubstrateAddress::new(public_key.to_bytes(), SubstrateCryptoScheme::Sr25519)?;

            if address.as_ss58().starts_with(prefix) {
                tracing::info!(
                    "Vanity key found after {} attempts for prefix '{}'",
                    attempts,
                    prefix
                );
                return match password {
                    Some(pwd) => Ok(self.import_encrypted(candidate, pwd)?),
                    None => Ok(self.import(candidate)?),
                };
            }

            if attempts.is_multiple_of(1000) {
                tracing::info!(
                    "Still searching vanity key, attempts: {}, prefix: '{}'",
                    attempts,
                    prefix
                );
            }
        }
    }
}
