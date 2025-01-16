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

use ethexe_common::events::{BlockEvent, BlockRequestEvent};
use ethexe_db::BlockHeader;
use gprimitives::{CodeId, H256};
use parity_scale_codec::{Decode, Encode};

#[derive(Debug, Clone, Encode, Decode)]
pub enum RequestEvent {
    Block(RequestBlockData),
    CodeLoaded { code_id: CodeId, code: Vec<u8> },
}

#[derive(Debug, Clone, Encode, Decode)]
pub enum Event {
    Block(BlockData),
    CodeLoaded { code_id: CodeId, code: Vec<u8> },
}

#[derive(Debug, Clone, Encode, Decode)]
pub struct RequestBlockData {
    pub hash: H256,
    pub header: BlockHeader,
    pub events: Vec<BlockRequestEvent>,
}

impl RequestBlockData {
    pub fn as_simple(&self) -> SimpleBlockData {
        SimpleBlockData {
            hash: self.hash,
            header: self.header.clone(),
        }
    }
}

#[derive(Debug, Clone, Encode, Decode)]
pub struct BlockData {
    pub hash: H256,
    pub header: BlockHeader,
    pub events: Vec<BlockEvent>,
}

impl BlockData {
    pub fn as_simple(&self) -> SimpleBlockData {
        SimpleBlockData {
            hash: self.hash,
            header: self.header.clone(),
        }
    }
}

#[derive(Debug, Clone, Encode, Decode)]
pub struct SimpleBlockData {
    pub hash: H256,
    pub header: BlockHeader,
}
