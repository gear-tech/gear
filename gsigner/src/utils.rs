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

//! Small utility helpers used across the crate.

use hex::FromHexError;

/// Decode a hex string (with optional `0x` prefix) into a fixed-size array.
pub fn decode_hex_to_array<const N: usize>(s: &str) -> Result<[u8; N], FromHexError> {
    let stripped = s.strip_prefix("0x").unwrap_or(s);
    if stripped.len() != N * 2 {
        return Err(FromHexError::InvalidStringLength);
    }

    let mut buf = [0u8; N];
    hex::decode_to_slice(stripped, &mut buf)?;
    Ok(buf)
}
