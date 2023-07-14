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

use crate::{Block, BlockHashOrNumber, HashFor, NumberFor};

pub(crate) fn hash(block_hash: &str) -> Result<String, usize> {
    let (block_hash, offset) = if let Some(block_hash) = block_hash.strip_prefix("0x") {
        (block_hash, 2)
    } else {
        (block_hash, 0)
    };

    if let Some(pos) = block_hash.chars().position(|c| !c.is_ascii_hexdigit()) {
        Err(offset + pos)
    } else {
        Ok(block_hash.into())
    }
}

pub(crate) fn url(s: &str) -> Result<String, &'static str> {
    if s.starts_with("ws://") || s.starts_with("wss://") {
        Ok(s.to_string())
    } else {
        Err("not a valid WS(S) url: must start with 'ws://' or 'wss://'")
    }
}

pub(crate) fn block(block_hash_or_number: &str) -> Result<BlockHashOrNumber<Block>, String> {
    if let Ok(block_number) = block_hash_or_number.parse::<NumberFor<Block>>() {
        Ok(BlockHashOrNumber::Number(block_number))
    } else {
        let block_hash = hash(block_hash_or_number).map_err(|e| {
            format!("Expected block hash or number, found illegal hex character at position {e}")
        })?;
        if block_hash.len() != 64 {
            return Err(format!(
                "Expected block hash or number, found a hex string of length {}",
                block_hash.len()
            ));
        };
        block_hash
            .parse::<HashFor<Block>>()
            .map(BlockHashOrNumber::Hash)
            .map_err(|e| format!("Failed to parse block hash: {e}"))
    }
}
