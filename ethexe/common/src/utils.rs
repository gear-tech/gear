// This file is part of Gear.
//
// Copyright (C) 2025 Gear Technologies Inc.
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

/// Decodes hexed string to a byte array.
pub fn decode_to_array<const N: usize>(s: &str) -> Result<[u8; N], hex::FromHexError> {
    // Strip the "0x" prefix if it exists.
    let stripped = s.strip_prefix("0x").unwrap_or(s);

    // Decode
    let mut buf = [0u8; N];
    hex::decode_to_slice(stripped, &mut buf)?;

    Ok(buf)
}

/// Converts u64 to a 48-bit unsigned integer, represented as a byte array in big-endian order.
pub const fn u64_into_uint48_be_bytes_lossy(val: u64) -> [u8; 6] {
    let [_, _, b1, b2, b3, b4, b5, b6] = val.to_be_bytes();

    [b1, b2, b3, b4, b5, b6]
}
