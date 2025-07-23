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

//! codec for keys.

use anyhow::{Result, anyhow};
use rand::RngCore;
use schnorrkel::PUBLIC_KEY_LENGTH;

/// Parameters of scrypt
pub struct Scrypt {
    /// Salt used for scrypt.
    pub salt: [u8; Self::SALT_LENGTH],
    /// CPU/memory cost parameter, must be power of 2 (e.g. 1024).
    pub n: u32,
    /// Block size parameter, which fine-tunes sequential memory
    /// read size and performance ( 8 is commonly used ).
    pub r: u32,
    /// Parallelization parameter ( 1 .. 2^32 -1 * hLen/MFlen ).
    pub p: u32,
}

impl Scrypt {
    /// The length of encoded scrypt params.
    ///
    /// NOTE: SALT(32) + N(4) + R(4) + P(4)
    pub const ENCODED_LENGTH: usize = 44;

    /// The length of salt used for scrypt.
    const SALT_LENGTH: usize = 32;

    /// Read from encoded data.
    pub fn decode(encoded: [u8; Self::ENCODED_LENGTH]) -> Self {
        let mut salt = [0; Self::SALT_LENGTH];
        salt.copy_from_slice(&encoded[..Self::SALT_LENGTH]);

        let params = encoded[Self::SALT_LENGTH..]
            .chunks(4)
            .map(|bytes| {
                let mut buf = [0; 4];
                buf.copy_from_slice(bytes);
                u32::from_le_bytes(buf)
            })
            .collect::<Vec<_>>();

        Self {
            salt,
            n: params[0].ilog2(),
            r: params[2],
            p: params[1],
        }
    }

    /// Encode self to bytes.
    pub fn encode(&self) -> [u8; Self::ENCODED_LENGTH] {
        let mut buf = [0; Self::ENCODED_LENGTH];
        let n = 1 << self.n;
        buf[..Self::SALT_LENGTH].copy_from_slice(&self.salt);
        buf[Self::SALT_LENGTH..].copy_from_slice(
            [n, self.p, self.r]
                .iter()
                .flat_map(|n| n.to_le_bytes())
                .collect::<Vec<_>>()
                .as_slice(),
        );

        buf
    }

    /// Get passwd from passphrase.
    pub fn passwd(&self, passphrase: &[u8]) -> Result<[u8; 32]> {
        let mut passwd = [0; 32];
        let output = nacl::scrypt(
            passphrase,
            &self.salt,
            self.n as u8,
            self.r as usize,
            self.p as usize,
            PUBLIC_KEY_LENGTH,
            &|_: u32| {},
        )
        .map_err(|e| anyhow!("{e:?}"))?;
        passwd.copy_from_slice(&output[..32]);

        Ok(passwd)
    }
}

impl Default for Scrypt {
    fn default() -> Self {
        let mut salt = [0; Self::SALT_LENGTH];
        rand::thread_rng().fill_bytes(&mut salt);

        Self {
            salt,
            n: 15,
            r: 8,
            p: 1,
        }
    }
}
