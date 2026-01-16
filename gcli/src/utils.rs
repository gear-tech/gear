// This file is part of Gear.
//
// Copyright (C) 2021-2025 Gear Technologies Inc.
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

//! Gear program utils

use hex::FromHexError;
use std::str::FromStr;

/// Bytes with hex string representation.
#[derive(
    derive_more::Debug,
    Clone,
    derive_more::AsRef,
    derive_more::AsMut,
    derive_more::Deref,
    derive_more::DerefMut,
    derive_more::Into,
    derive_more::From,
    derive_more::Display,
)]
#[as_ref(forward)]
#[as_mut(forward)]
#[deref(forward)]
#[deref_mut(forward)]
#[display("0x{}", hex::encode(self))]
#[debug("0x{}", hex::encode(self))]
pub struct HexBytes(Vec<u8>);

impl HexBytes {
    /// Returns reference to its bytes.
    pub fn as_slice(&self) -> &[u8] {
        self
    }

    /// Returns reference to its bytes.
    pub fn as_slice_mut(&mut self) -> &mut [u8] {
        self
    }

    /// Converts to its bytes.
    pub fn into_vec(self) -> Vec<u8> {
        self.into()
    }
}

impl FromStr for HexBytes {
    type Err = FromHexError;

    fn from_str(s: &str) -> std::result::Result<Self, Self::Err> {
        hex::decode(s.trim_start_matches("0x")).map(Self)
    }
}
