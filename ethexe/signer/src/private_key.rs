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

//! Private key type.

use crate::utils;
use anyhow::{Error, Result};
use parity_scale_codec::{Decode, Encode};
use secp256k1::SecretKey as Secp256k1SecretKey;
use std::str::FromStr;

/// Private key.
///
/// Private key type used for elliptic curves maths for secp256k1 standard
/// is a 256 bits unsigned integer, which the type stores as a 32 bytes array.
#[derive(Encode, Decode, Default, Clone, Copy, PartialEq, Eq, Hash)]
pub struct PrivateKey(pub [u8; 32]);

impl From<PrivateKey> for Secp256k1SecretKey {
    fn from(key: PrivateKey) -> Self {
        Secp256k1SecretKey::from_byte_array(&key.0).expect("32 bytes; within curve order")
    }
}

impl FromStr for PrivateKey {
    type Err = Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Ok(Self(utils::decode_to_array(s)?))
    }
}
