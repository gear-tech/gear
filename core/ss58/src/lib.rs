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

//! SS58 encoding implementation
//!
//! This library is extracted from [ss58 codec][ss58-codec] in `sp-core`, not
//! importing `sp-core` because it is super big (~300 dependencies).
//!
//! [ss58-codec]: https://paritytech.github.io/polkadot-sdk/master/sp_core/crypto/trait.Ss58Codec.html

#![no_std]

#[cfg(feature = "alloc")]
extern crate alloc;

#[cfg(feature = "alloc")]
use alloc::{
    string::{String, ToString},
    vec::Vec,
};
use blake2::{Blake2b512, Digest};
use bs58::{
    decode::{self, DecodeTarget},
    encode::{self, EncodeTarget},
};
use core::{
    array::TryFromSliceError,
    fmt,
    ops::{Deref, DerefMut, RangeInclusive},
    str,
    sync::atomic::{AtomicU16, Ordering},
};

// Prefix for checksum.
const PREFIX: &[u8] = b"SS58PRE";

/// Allowed prefix length.
const PREFIX_LEN_RANGE: RangeInclusive<usize> = 1..=2;
/// Minimum prefix length.
const MIN_PREFIX_LEN: usize = *PREFIX_LEN_RANGE.start();
/// Maximum prefix length.
const MAX_PREFIX_LEN: usize = *PREFIX_LEN_RANGE.end();

/// Length of public key is 32 bytes.
const BODY_LEN: usize = 32;

/// Default checksum size is 2 bytes.
const CHECKSUM_LEN: usize = 2;

/// Minimum address length without base58 encoding.
const MIN_ADDRESS_LEN: usize = MIN_PREFIX_LEN + BODY_LEN + CHECKSUM_LEN;
/// Maximum address length without base58 encoding.
const MAX_ADDRESS_LEN: usize = MAX_PREFIX_LEN + BODY_LEN + CHECKSUM_LEN;
/// Allowed address length without base58 encoding.
const ADDRESS_LEN_RANGE: RangeInclusive<usize> = MIN_ADDRESS_LEN..=MAX_ADDRESS_LEN;

/// Function is taken from [`bs58`] to calculate the maximum length required for
/// base58 encoding.
const fn base58_max_encoded_len(len: usize) -> usize {
    // log_2(256) / log_2(58) ≈ 1.37.  Assume 1.5 for easier calculation.
    len + len.div_ceil(2)
}

/// Maximum address length in base58 encoding.
const MAX_ADDRESS_LEN_BASE58: usize = base58_max_encoded_len(MAX_ADDRESS_LEN);

/// The SS58 prefix of substrate.
pub const SUBSTRATE_SS58_PREFIX: u16 = 42;
/// The SS58 prefix of vara network.
pub const VARA_SS58_PREFIX: u16 = 137;

/// The default ss58 version.
static DEFAULT_SS58_VERSION: AtomicU16 = AtomicU16::new(VARA_SS58_PREFIX);

/// Get the default ss58 version.
pub fn default_ss58_version() -> u16 {
    DEFAULT_SS58_VERSION.load(Ordering::Relaxed)
}

/// Set the default ss58 version.
pub fn set_default_ss58_version(version: u16) {
    DEFAULT_SS58_VERSION.store(version, Ordering::Relaxed);
}

struct Buffer<const N: usize>([u8; N]);

impl<const N: usize> Buffer<N> {
    pub const fn new() -> Self {
        Self([0; N])
    }
}

impl<const N: usize> Deref for Buffer<N> {
    type Target = [u8];

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl<const N: usize> DerefMut for Buffer<N> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}

impl DecodeTarget for Buffer<MAX_ADDRESS_LEN> {
    fn decode_with(
        &mut self,
        _max_len: usize,
        f: impl for<'a> FnOnce(&'a mut [u8]) -> decode::Result<usize>,
    ) -> decode::Result<usize> {
        let len = f(&mut self[..])?;
        Ok(len)
    }
}

impl EncodeTarget for Buffer<MAX_ADDRESS_LEN_BASE58> {
    fn encode_with(
        &mut self,
        _max_len: usize,
        f: impl for<'a> FnOnce(&'a mut [u8]) -> encode::Result<usize>,
    ) -> encode::Result<usize> {
        let len = f(&mut self[..])?;
        Ok(len)
    }
}

/// An error type for SS58 decoding.
#[derive(Debug, PartialEq, Eq)]
pub enum Error {
    Base58Encode,
    BadBase58,
    BadLength,
    InvalidPrefix,
    InvalidChecksum,
    #[cfg(feature = "alloc")]
    InvalidSliceLength,
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Base58Encode => writeln!(f, "Base 58 encoding failed"),
            Self::BadBase58 => writeln!(f, "Base 58 requirement is violated"),
            Self::BadLength => writeln!(f, "Length is bad"),
            Self::InvalidPrefix => writeln!(f, "Invalid SS58 prefix byte"),
            Self::InvalidChecksum => writeln!(f, "Invalid checksum"),
            #[cfg(feature = "alloc")]
            Self::InvalidSliceLength => writeln!(f, "Slice should be 32 length"),
        }
    }
}

impl core::error::Error for Error {}

/// Represents SS58 address.
pub struct Ss58Address {
    len: usize,
    buf: Buffer<MAX_ADDRESS_LEN_BASE58>,
}

impl Ss58Address {
    /// Returns string slice containing SS58 address.
    pub fn as_str(&self) -> &str {
        unsafe { str::from_utf8_unchecked(&self.buf).get_unchecked(..self.len) }
    }
}

impl fmt::Display for Ss58Address {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

impl fmt::Debug for Ss58Address {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Display::fmt(self, f)
    }
}

/// Represents public key bytes.
pub struct RawSs58Address([u8; BODY_LEN]);

impl From<RawSs58Address> for [u8; BODY_LEN] {
    fn from(address: RawSs58Address) -> Self {
        address.0
    }
}

impl From<[u8; BODY_LEN]> for RawSs58Address {
    fn from(array: [u8; BODY_LEN]) -> Self {
        Self(array)
    }
}

impl TryFrom<&[u8]> for RawSs58Address {
    type Error = TryFromSliceError;

    fn try_from(slice: &[u8]) -> Result<Self, Self::Error> {
        <[u8; BODY_LEN]>::try_from(slice).map(Self)
    }
}

impl fmt::Display for RawSs58Address {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let mut buf = [0; BODY_LEN * 2];
        let _ = hex::encode_to_slice(self.0, &mut buf);
        f.write_str("0x")?;
        f.write_str(unsafe { str::from_utf8_unchecked(&buf) })
    }
}

impl fmt::Debug for RawSs58Address {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Display::fmt(self, f)
    }
}

impl RawSs58Address {
    /// Returns raw address if string is properly encoded ss58-check address.
    pub fn from_ss58check(s: &str) -> Result<Self, Error> {
        Self::from_ss58check_with_prefix(s).map(|(address, _)| address)
    }

    /// Returns raw address with prefix if string is properly encoded ss58-check address.
    pub fn from_ss58check_with_prefix(s: &str) -> Result<(Self, u16), Error> {
        let mut data = Buffer::<MAX_ADDRESS_LEN>::new();
        let data_len = bs58::decode(s)
            .onto(&mut data)
            .map_err(|_| Error::BadBase58)?;

        if !ADDRESS_LEN_RANGE.contains(&data_len) {
            return Err(Error::BadLength);
        }

        let (prefix_len, prefix) = match data[0] {
            0..=63 => (1, data[0] as u16),
            64..=127 => {
                // weird bit manipulation owing to the combination of LE encoding and missing two
                // bits from the left.
                // d[0] d[1] are: 01aaaaaa bbcccccc
                // they make the LE-encoded 16-bit value: aaaaaabb 00cccccc
                // so the lower byte is formed of aaaaaabb and the higher byte is 00cccccc
                let lower = (data[0] << 2) | (data[1] >> 6);
                let upper = data[1] & 0b00111111;
                (2, (lower as u16) | ((upper as u16) << 8))
            }
            _ => return Err(Error::InvalidPrefix),
        };

        if data_len != prefix_len + BODY_LEN + CHECKSUM_LEN {
            return Err(Error::BadLength);
        }

        let (address_data, address_checksum) = data.split_at(prefix_len + BODY_LEN);

        let hash = ss58hash(address_data);
        let checksum = &hash[..CHECKSUM_LEN];

        if &address_checksum[..CHECKSUM_LEN] != checksum {
            return Err(Error::InvalidChecksum);
        }

        match <[u8; BODY_LEN]>::try_from(&address_data[prefix_len..]) {
            Ok(array) => Ok((Self(array), prefix)),
            Err(_) => Err(Error::BadLength),
        }
    }
}

impl RawSs58Address {
    /// Returns ss58-check string for this address. The prefix can be overridden via [`default_ss58_version()`].
    pub fn to_ss58check(&self) -> Result<Ss58Address, Error> {
        self.to_ss58check_with_prefix(default_ss58_version())
    }

    /// Returns ss58-check string for this address with given prefix.
    pub fn to_ss58check_with_prefix(&self, prefix: u16) -> Result<Ss58Address, Error> {
        let mut buffer = Buffer::<MAX_ADDRESS_LEN>::new();

        // We mask out the upper two bits of the ident - SS58 Prefix currently only supports 14-bits
        let ident = prefix & 0b0011_1111_1111_1111;
        let (prefix_len, address_len) = match ident {
            0..=63 => {
                buffer[0] = ident as u8;
                (MIN_PREFIX_LEN, MIN_ADDRESS_LEN)
            }
            64..=16_383 => {
                // upper six bits of the lower byte(!)
                let first = ((ident & 0b0000_0000_1111_1100) as u8) >> 2;
                // lower two bits of the lower byte in the high pos,
                // lower bits of the upper byte in the low pos
                let second = ((ident >> 8) as u8) | (((ident & 0b0000_0000_0000_0011) as u8) << 6);

                buffer[0] = first | 0b01000000;
                buffer[1] = second;
                (MAX_PREFIX_LEN, MAX_ADDRESS_LEN)
            }
            _ => unreachable!("masked out the upper two bits; qed"),
        };

        let (address_data, address_checksum) = buffer.split_at_mut(prefix_len + BODY_LEN);

        address_data[prefix_len..].copy_from_slice(&self.0);
        let hash = ss58hash(address_data);
        address_checksum[..CHECKSUM_LEN].copy_from_slice(&hash[..CHECKSUM_LEN]);

        let mut buf = Buffer::<MAX_ADDRESS_LEN_BASE58>::new();
        let len = bs58::encode(&buffer[..address_len])
            .onto(&mut buf)
            .map_err(|_| Error::Base58Encode)?;

        Ok(Ss58Address { len, buf })
    }
}

fn ss58hash(data: &[u8]) -> [u8; 64] {
    let mut ctx = Blake2b512::new();
    ctx.update(PREFIX);
    ctx.update(data);
    ctx.finalize().into()
}

/// Encode data to SS58 format.
#[cfg(feature = "alloc")]
pub fn encode(data: &[u8]) -> Result<String, Error> {
    let raw_address = RawSs58Address::try_from(data).map_err(|_| Error::InvalidSliceLength)?;
    let address = raw_address.to_ss58check()?;
    Ok(address.to_string())
}

/// Decode data from SS58 format.
#[cfg(feature = "alloc")]
pub fn decode(encoded: &str) -> Result<Vec<u8>, Error> {
    let raw_address: [u8; BODY_LEN] = RawSs58Address::from_ss58check(encoded)?.into();
    Ok(raw_address.to_vec())
}

/// Re-encoding a ss58 address in the current [`default_ss58_version()`].
#[cfg(feature = "alloc")]
pub fn recode(encoded: &str) -> Result<String, Error> {
    self::encode(&self::decode(encoded)?)
}
