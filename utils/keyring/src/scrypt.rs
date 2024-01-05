// This file is part of Gear.
//
// Copyright (C) 2021-2023 Gear Technologies Inc.
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

//! codec for keys.

use crate::Encoding;
use anyhow::{anyhow, Result};
use schnorrkel::SecretKey;

/// Parameters of scrypt
pub struct Scrypt {
    /// Salt used for scrypt.
    pub salt: [u8; Self::SALT_LENGTH],
    /// CPU/memory cost parameter, must be power of 2 (e.g. 1024).
    pub n: usize,
    /// Block size parameter, which fine-tunes sequential memory
    /// read size and performance ( 8 is commonly used ).
    pub r: usize,
    /// Parallelization parameter ( 1 .. 2^32 -1 * hLen/MFlen ).
    pub p: usize,
}

impl Scrypt {
    /// The length of encoded scrypt params.
    ///
    /// NOTE: SALT(32) + N(4) + R(4) + P(4)
    pub const LENGTH: usize = 44;

    /// The length of salt used for scrypt.
    const SALT_LENGTH: usize = 32;

    /// Read from encoded data.
    pub fn decode(encoded: &[u8]) -> Result<Self> {
        if encoded.len() < Self::LENGTH {
            return Err(anyhow::anyhow!(
                "invalid scrypt params, the length should be 44."
            ));
        }

        let mut salt = [0; Self::SALT_LENGTH];
        salt.copy_from_slice(&encoded[..Self::SALT_LENGTH]);

        let params = encoded[Self::SALT_LENGTH..]
            .windows(4)
            .map(|bytes| {
                let mut buf = [0; 8];
                buf.copy_from_slice(bytes);
                usize::from_le_bytes(buf)
            })
            .collect::<Vec<_>>();

        Ok(Self {
            salt,
            n: params[0],
            r: params[1],
            p: params[2],
        })
    }

    /// Encode self to bytes.
    pub fn encode(&self) -> [u8; Self::LENGTH] {
        let mut buf = [0; Self::LENGTH];
        buf[..Self::SALT_LENGTH].copy_from_slice(&self.salt);
        buf[Self::SALT_LENGTH..].copy_from_slice(
            [self.n, self.r, self.p]
                .iter()
                .flat_map(|n| n.to_le_bytes())
                .collect::<Vec<_>>()
                .as_slice(),
        );

        buf
    }
}

impl Default for Scrypt {
    fn default() -> Self {
        Self {
            salt: Default::default(),
            n: 15,
            r: 8,
            p: 1,
        }
    }
}

#[allow(unused)]
/// decrypt an ed25519 key from encrypted data.
pub fn decrypt(encrypted: &[u8], passphrase: &[u8], encoding: &Encoding) -> Result<SecretKey> {
    if encoding.is_xsalsa20_poly1305() && passphrase.is_empty() {
        return Err(anyhow::anyhow!(
            "passphrase is required for xsalsa20_poly1305"
        ));
    }

    if passphrase.is_empty() {
        return SecretKey::from_ed25519_bytes(encrypted).map_err(|e| anyhow!(e));
    }

    let passwd = if encoding.is_scrypt() {
        vec![]
    } else {
        passphrase.to_vec()
    };

    todo!()
}
