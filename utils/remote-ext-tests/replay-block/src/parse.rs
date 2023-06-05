// This file is part of Gear.

// Copyright (C) 2021-2022 Gear Technologies Inc.
// SPDX-License-Identifier: GPL-3.0-or-later WITH Classpath-exception-2.0

// This program is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.

// This program is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE. See the
// GNU General Public License for more details.

// You should have received a copy of the GNU General Public License
// along with this program. If not, see <https://www.gnu.org/licenses/>.

//! Utils for parsing input args

pub(crate) fn hash(block_hash: &str) -> Result<String, String> {
    let (block_hash, offset) = if let Some(block_hash) = block_hash.strip_prefix("0x") {
        (block_hash, 2)
    } else {
        (block_hash, 0)
    };

    if let Some(pos) = block_hash.chars().position(|c| !c.is_ascii_hexdigit()) {
        Err(format!(
            "Expected block hash, found illegal hex character at position: {}",
            offset + pos,
        ))
    } else {
        Ok(block_hash.into())
    }
}

pub(crate) fn url(s: &str) -> Result<String, &'static str> {
    if s.starts_with("ws://") || s.starts_with("wss://") {
        // could use Url crate as well, but lets keep it simple for now.
        Ok(s.to_string())
    } else {
        Err("not a valid WS(S) url: must start with 'ws://' or 'wss://'")
    }
}
