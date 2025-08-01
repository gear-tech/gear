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

use crate::Keypair;
use anyhow::{Result, anyhow};
use schnorrkel::{KEYPAIR_LENGTH, PUBLIC_KEY_LENGTH, SECRET_KEY_LENGTH};

/// Key info wrapped in pkcs8 format.
///
/// NOTE: the meaning of these bytes is ambiguous for now, see
/// <https://github.com/polkadot-js/common/issues/1908>
///
/// For the encoded data format of this implementation:
///
/// ENCODED(117) = HEADER(16) + SECRET_KEY_LENGTH(64) + DIVIDER(5) + PUBLIC_KEY_LENGTH(32)
pub struct KeypairInfo {
    /// Schnorrkel secret key.
    pub secret: [u8; SECRET_KEY_LENGTH],
    /// Schnorrkel public key.
    pub public: [u8; PUBLIC_KEY_LENGTH],
}

impl KeypairInfo {
    /// The length of the pkcs8 key info.
    ///
    /// NOTE: LENGTH(117) = HEADER(16) + SECRET_KEY_LENGTH + DIVIDER(5) + PUBLIC_KEY_LENGTH
    pub const ENCODED_LENGTH: usize = 117;

    /// The length of pkcs8 header in polkadot-js.
    const PKCS8_HEADER_LENGTH: usize = 16;

    /// The pkcs8 header used in polkadot-js.
    const PKCS8_HEADER: [u8; Self::PKCS8_HEADER_LENGTH] =
        [48, 83, 2, 1, 1, 48, 5, 6, 3, 43, 101, 112, 4, 34, 4, 32];

    /// The length of pkcs8 divider in polkadot-js.
    const PKCS8_DIVIDER_LENGTH: usize = 5;

    /// The pkcs8 divider used in polkadot-js.
    const PKCS8_DIVIDER: [u8; Self::PKCS8_DIVIDER_LENGTH] = [161, 35, 3, 33, 0];

    /// The offset of secret key in pkcs8 key info.
    const SECRET_KEY_OFFSET: usize = Self::PKCS8_HEADER_LENGTH;

    /// The offset of divider in pkcs8 key info.
    const PKCS8_DIVIDER_OFFSET: usize = Self::PKCS8_HEADER_LENGTH + SECRET_KEY_LENGTH;

    /// The offset of public key in pkcs8 key info.
    const PUBLIC_KEY_OFFSET: usize =
        Self::SECRET_KEY_OFFSET + SECRET_KEY_LENGTH + Self::PKCS8_DIVIDER_LENGTH;

    /// Decode key info from fixed bytes.
    pub fn decode(data: &[u8]) -> Result<Self> {
        if data[..Self::PKCS8_HEADER_LENGTH] != Self::PKCS8_HEADER {
            return Err(anyhow!("invalid pkcs8 header"));
        }

        if data[Self::PKCS8_DIVIDER_OFFSET..Self::PKCS8_DIVIDER_OFFSET + Self::PKCS8_DIVIDER_LENGTH]
            != Self::PKCS8_DIVIDER
        {
            return Err(anyhow!("invalid pkcs8 divider"));
        }

        let mut encoded = [0; Self::ENCODED_LENGTH];
        encoded.copy_from_slice(data);

        let mut secret = [0u8; SECRET_KEY_LENGTH];
        let mut public = [0u8; PUBLIC_KEY_LENGTH];

        secret.copy_from_slice(
            &encoded[Self::SECRET_KEY_OFFSET..Self::SECRET_KEY_OFFSET + SECRET_KEY_LENGTH],
        );
        public.copy_from_slice(
            &encoded[Self::PUBLIC_KEY_OFFSET..Self::PUBLIC_KEY_OFFSET + PUBLIC_KEY_LENGTH],
        );

        Ok(Self { secret, public })
    }

    /// Encode self to fixed bytes.
    pub fn encode(&self) -> [u8; Self::ENCODED_LENGTH] {
        let mut encoded = [0; Self::ENCODED_LENGTH];

        encoded[..Self::PKCS8_HEADER_LENGTH].copy_from_slice(&Self::PKCS8_HEADER);
        encoded[Self::SECRET_KEY_OFFSET..Self::SECRET_KEY_OFFSET + SECRET_KEY_LENGTH]
            .copy_from_slice(&self.secret);
        encoded
            [Self::PKCS8_DIVIDER_OFFSET..Self::PKCS8_DIVIDER_OFFSET + Self::PKCS8_DIVIDER_LENGTH]
            .copy_from_slice(&Self::PKCS8_DIVIDER);
        encoded[Self::PUBLIC_KEY_OFFSET..Self::PUBLIC_KEY_OFFSET + PUBLIC_KEY_LENGTH]
            .copy_from_slice(&self.public);

        encoded
    }

    /// Convert self to schnorrkel keypair.
    pub fn into_keypair(self) -> Result<Keypair> {
        let mut bytes = [0u8; KEYPAIR_LENGTH];
        bytes[..SECRET_KEY_LENGTH].copy_from_slice(&self.secret);
        bytes[SECRET_KEY_LENGTH..].copy_from_slice(&self.public);
        Keypair::from_half_ed25519_bytes(&bytes)
            .map_err(|e| anyhow!("Failed to create pair: {e:?}"))
    }
}

impl From<Keypair> for KeypairInfo {
    fn from(keypair: Keypair) -> Self {
        let secret = keypair.secret.to_ed25519_bytes();
        let public = keypair.public.to_bytes();
        Self { secret, public }
    }
}
