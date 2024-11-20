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

use anyhow::{anyhow, Result};

pub(crate) fn decode_to_array<const N: usize>(s: &str) -> Result<[u8; N]> {
    let mut buf = [0; N];
    hex::decode_to_slice(strip_prefix(s), &mut buf)
        .map_err(|_| anyhow!("invalid hex format for {s:?}"))?;
    Ok(buf)
}

pub(crate) fn strip_prefix(s: &str) -> &str {
    if let Some(s) = s.strip_prefix("0x") {
        s
    } else {
        s
    }
}
