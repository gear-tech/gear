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

use crate::errors;
use ethexe_common::{
    db::{BlockMetaStorageRead, OnChainStorageRead},
    BlockHeader,
};
use gprimitives::H256;
use jsonrpsee::core::RpcResult;

pub fn block_header_at_or_latest<DB: BlockMetaStorageRead + OnChainStorageRead>(
    db: &DB,
    at: impl Into<Option<H256>>,
) -> RpcResult<(H256, BlockHeader)> {
    if let Some(hash) = at.into() {
        db.block_header(hash)
            .map(|header| (hash, header))
            .ok_or_else(|| errors::db("Block header for requested hash wasn't found"))
    } else {
        db.latest_computed_block()
            .ok_or_else(|| errors::db("Latest block header wasn't found"))
    }
}
