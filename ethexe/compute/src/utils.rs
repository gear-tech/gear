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

use crate::{ComputeError, Result};
use ethexe_common::{
    db::{BlockMetaStorageRead, OnChainStorageRead},
    BlockMeta, SimpleBlockData,
};
use gprimitives::H256;
use std::collections::VecDeque;

/// Collect a chain of blocks from the head to the last block that satisfies the filter.
/// Stops when the filter returns false for the block meta.
/// Returns a chain sorted in order from the oldest to the newest block (head is newest).
pub fn collect_chain<DB: BlockMetaStorageRead + OnChainStorageRead>(
    db: &DB,
    head: H256,
    mut filter: impl FnMut(&BlockMeta) -> bool,
) -> Result<VecDeque<SimpleBlockData>> {
    let mut block = head;
    let mut chain = VecDeque::new();

    while filter(&db.block_meta(block)) {
        let header = db
            .block_header(block)
            .ok_or(ComputeError::BlockHeaderNotFound(block))?;

        let parent = header.parent_hash;

        chain.push_front(SimpleBlockData {
            hash: block,
            header,
        });

        block = parent;
    }

    Ok(chain)
}
