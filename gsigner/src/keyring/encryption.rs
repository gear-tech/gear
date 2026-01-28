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

//! Shared helpers for password-based keystore encryption.

use anyhow::{Result, anyhow};
use base64::{Engine as _, engine::general_purpose::STANDARD};
use rand::RngCore;

const NONCE_LENGTH: usize = 24;
const KEY_SIZE: usize = 32;
const SCRYPT_LOG_N_MIN: u32 = 10; // 1 << 10 = 1024
const SCRYPT_LOG_N_MAX: u32 = 20; // 1 << 20 = 1,048,576
const SCRYPT_R_MAX: u32 = 8; // keep memory use bounded
const SCRYPT_P_MAX: u32 = 8; // keep CPU/memory bounded

/// Encrypt arbitrary bytes with scrypt + xsalsa20-poly1305.
pub fn encrypt_secret(plaintext: &[u8], passphrase: &[u8]) -> Result<String> {
    let scrypt = Scrypt::default();
    let passwd = scrypt.passwd(passphrase)?;

    let mut encoded = Vec::with_capacity(Scrypt::ENCODED_LENGTH + NONCE_LENGTH + plaintext.len());
    encoded.extend_from_slice(&scrypt.encode());

    let mut nonce = [0u8; NONCE_LENGTH];
    rand::thread_rng().fill_bytes(&mut nonce);
    encoded.extend_from_slice(&nonce);

    let ciphertext = nacl::secret_box::pack(plaintext, &nonce, &passwd[..KEY_SIZE])
        .map_err(|e| anyhow!("{:?}", e))?;
    encoded.extend_from_slice(&ciphertext);

    Ok(STANDARD.encode(encoded))
}

/// Decrypt bytes previously produced by [`encrypt_secret`].
pub fn decrypt_secret(encoded: &str, passphrase: &[u8]) -> Result<Vec<u8>> {
    let decoded = STANDARD.decode(encoded)?;
    if decoded.len() < Scrypt::ENCODED_LENGTH + NONCE_LENGTH {
        anyhow::bail!("Invalid encrypted payload");
    }

    let mut scrypt_bytes = [0u8; Scrypt::ENCODED_LENGTH];
    scrypt_bytes.copy_from_slice(&decoded[..Scrypt::ENCODED_LENGTH]);
    let scrypt = Scrypt::decode(scrypt_bytes)?;
    let passwd = scrypt.passwd(passphrase)?;

    let encrypted = &decoded[Scrypt::ENCODED_LENGTH..];
    let nonce = &encrypted[..NONCE_LENGTH];
    let payload = &encrypted[NONCE_LENGTH..];

    let secret = nacl::secret_box::open(payload, nonce, &passwd[..KEY_SIZE])
        .map_err(|e| anyhow!("{:?}", e))?;
    Ok(secret)
}

/// Scrypt parameters used for deriving the symmetric key.
struct Scrypt {
    salt: [u8; 32],
    n: u32,
    r: u32,
    p: u32,
}

impl Scrypt {
    const ENCODED_LENGTH: usize = 44;

    fn encode(&self) -> [u8; Self::ENCODED_LENGTH] {
        let mut buf = [0u8; Self::ENCODED_LENGTH];
        let n: u32 = 1 << self.n;
        buf[..32].copy_from_slice(&self.salt);
        buf[32..36].copy_from_slice(&n.to_le_bytes());
        buf[36..40].copy_from_slice(&self.p.to_le_bytes());
        buf[40..44].copy_from_slice(&self.r.to_le_bytes());
        buf
    }

    fn decode(encoded: [u8; Self::ENCODED_LENGTH]) -> Result<Self> {
        let mut salt = [0u8; 32];
        salt.copy_from_slice(&encoded[..32]);

        let params = encoded[32..]
            .chunks(4)
            .map(|bytes| {
                let mut buf = [0u8; 4];
                buf.copy_from_slice(bytes);
                u32::from_le_bytes(buf)
            })
            .collect::<Vec<_>>();

        let (n_raw, p, r) = match (params.first(), params.get(1), params.get(2)) {
            (Some(&n_raw), Some(&p), Some(&r)) => (n_raw, p, r),
            _ => anyhow::bail!("Invalid scrypt parameter block"),
        };

        if !n_raw.is_power_of_two() {
            anyhow::bail!("Invalid scrypt N value (must be power of two)");
        }

        let n_log2 = n_raw.trailing_zeros();
        if !(SCRYPT_LOG_N_MIN..=SCRYPT_LOG_N_MAX).contains(&n_log2) {
            anyhow::bail!("Unsupported scrypt N: 2^{n_log2}");
        }

        if r == 0 || r > SCRYPT_R_MAX {
            anyhow::bail!("Unsupported scrypt r parameter");
        }

        if p == 0 || p > SCRYPT_P_MAX {
            anyhow::bail!("Unsupported scrypt p parameter");
        }

        Ok(Self {
            salt,
            n: n_log2,
            r,
            p,
        })
    }

    fn passwd(&self, passphrase: &[u8]) -> Result<[u8; KEY_SIZE]> {
        let mut passwd = [0u8; KEY_SIZE];
        let output = nacl::scrypt(
            passphrase,
            &self.salt,
            self.n as u8,
            self.r as usize,
            self.p as usize,
            KEY_SIZE,
            &|_| {},
        )
        .map_err(|e| anyhow!("{:?}", e))?;
        passwd.copy_from_slice(&output[..KEY_SIZE]);
        Ok(passwd)
    }
}

impl Default for Scrypt {
    fn default() -> Self {
        let mut salt = [0u8; 32];
        rand::thread_rng().fill_bytes(&mut salt);

        // TODO: maybe lower n param, now it takes 3s to encrypt or decrypt keystore
        Self {
            salt,
            n: 15,
            r: 8,
            p: 1,
        }
    }
}
