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

//! Sr25519-specific signer extensions.

use super::{PublicKey, Signature, Sr25519};
use crate::{Signer, traits::SignatureScheme};
use anyhow::Result;
use schnorrkel::signing_context;

/// Extension trait for Sr25519 signers.
pub trait Sr25519SignerExt {
    /// Sign with a custom signing context.
    fn sign_with_context(
        &self,
        public_key: PublicKey,
        context: &[u8],
        data: &[u8],
    ) -> Result<Signature>;

    /// Verify with a custom signing context.
    fn verify_with_context(
        &self,
        public_key: PublicKey,
        context: &[u8],
        data: &[u8],
        signature: &Signature,
    ) -> Result<()>;

    /// Generate a vanity key with the specified SS58 prefix.
    fn generate_vanity_key(&self, prefix: &str) -> Result<PublicKey>;
}

impl Sr25519SignerExt for Signer<Sr25519> {
    fn sign_with_context(
        &self,
        public_key: PublicKey,
        context: &[u8],
        data: &[u8],
    ) -> Result<Signature> {
        let private_key = self.get_private_key(public_key)?;
        let ctx = signing_context(context);
        let keypair = private_key.keypair()?;
        let signature = keypair.sign(ctx.bytes(data));
        Ok(Signature::from(signature))
    }

    fn verify_with_context(
        &self,
        public_key: PublicKey,
        context: &[u8],
        data: &[u8],
        signature: &Signature,
    ) -> Result<()> {
        let ctx = signing_context(context);
        let schnorrkel_pub = schnorrkel::PublicKey::try_from(public_key)?;
        let schnorrkel_sig = schnorrkel::Signature::try_from(*signature)?;

        schnorrkel_pub
            .verify(ctx.bytes(data), &schnorrkel_sig)
            .map_err(|e| anyhow::anyhow!("Verification failed: {:?}", e))
    }

    fn generate_vanity_key(&self, prefix: &str) -> Result<PublicKey> {
        use crate::{address::SubstrateAddress, schemes::sr25519::PrivateKey};

        loop {
            let candidate = PrivateKey::random();
            let public_key = Sr25519::public_key(&candidate);
            let address = SubstrateAddress::new(public_key.to_bytes())?;

            if address.as_ss58().starts_with(prefix) {
                return Ok(self.import_key(candidate)?);
            }
        }
    }
}
