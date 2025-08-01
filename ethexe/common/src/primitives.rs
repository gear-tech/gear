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

use crate::{Digest, ToDigest, events::BlockEvent};
use alloc::{
    collections::{btree_map::BTreeMap, btree_set::BTreeSet},
    vec::Vec,
};
use gear_core::ids::prelude::CodeIdExt as _;
use gprimitives::{ActorId, CodeId, H256, MessageId};
use parity_scale_codec::{Decode, Encode};
use sha3::Digest as _;

pub type ProgramStates = BTreeMap<ActorId, StateHashWithQueueSize>;

#[derive(Debug, Clone, Default, Encode, Decode, PartialEq, Eq)]
#[cfg_attr(feature = "std", derive(serde::Serialize, serde::Deserialize))]
pub struct BlockHeader {
    pub height: u32,
    pub timestamp: u64,
    pub parent_hash: H256,
}

impl BlockHeader {
    pub fn dummy(height: u32) -> Self {
        let mut parent_hash = [0; 32];
        parent_hash[..4].copy_from_slice(&height.to_le_bytes());

        Self {
            height,
            timestamp: height as u64 * 12,
            parent_hash: parent_hash.into(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BlockData {
    pub hash: H256,
    pub header: BlockHeader,
    pub events: Vec<BlockEvent>,
}

impl BlockData {
    pub fn to_simple(&self) -> SimpleBlockData {
        SimpleBlockData {
            hash: self.hash,
            header: self.header.clone(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SimpleBlockData {
    pub hash: H256,
    pub header: BlockHeader,
}

#[derive(Clone, Copy, Debug, Default, Encode, Decode, PartialEq, Eq)]
pub struct BlockMeta {
    pub synced: bool,
    pub prepared: bool,
    pub computed: bool,
    pub last_committed_batch: Option<Digest>,
    pub last_committed_head: Option<H256>,
}

#[derive(Clone, Debug, Encode, Decode, PartialEq, Eq)]
pub struct ProducerBlock {
    pub block_hash: H256,
    pub gas_allowance: Option<u64>,
    pub off_chain_transactions: Vec<H256>,
}

impl ToDigest for ProducerBlock {
    fn update_hasher(&self, hasher: &mut sha3::Keccak256) {
        hasher.update(self.block_hash);
        hasher.update(self.gas_allowance.encode());
        hasher.update(self.off_chain_transactions.encode());
    }
}

#[derive(PartialEq, Eq, Debug, Clone, Copy, Default, Encode, Decode)]
#[cfg_attr(feature = "std", derive(serde::Serialize))]
pub struct StateHashWithQueueSize {
    pub hash: H256,
    pub cached_queue_size: u8,
}

impl StateHashWithQueueSize {
    pub fn zero() -> Self {
        Self {
            hash: H256::zero(),
            cached_queue_size: 0,
        }
    }
}

#[derive(Debug, Clone, Default, Encode, Decode, PartialEq, Eq)]
pub struct CodeBlobInfo {
    pub timestamp: u64,
    pub tx_hash: H256,
}

#[derive(Clone, PartialEq, Eq, derive_more::Debug)]
pub struct CodeAndIdUnchecked {
    #[debug("{:#x} bytes", code.len())]
    pub code: Vec<u8>,
    pub code_id: CodeId,
}

#[derive(Clone, PartialEq, Eq, derive_more::Debug)]
pub struct CodeAndId {
    #[debug("{:#x} bytes", code.len())]
    code: Vec<u8>,
    code_id: CodeId,
}

impl CodeAndId {
    pub fn new(code: Vec<u8>) -> Self {
        let code_id = CodeId::generate(&code);
        Self { code, code_id }
    }

    pub fn code(&self) -> &[u8] {
        &self.code
    }

    pub fn code_id(&self) -> CodeId {
        self.code_id
    }

    /// Creates a new `CodeAndId` from an unchecked version, asserting that the `code_id` matches the generated one.
    /// # Panics
    ///
    /// If the `code_id` does not match the generated one from the `code`, this function will panic.
    pub fn from_unchecked(code_and_id: CodeAndIdUnchecked) -> Self {
        let CodeAndIdUnchecked { code, code_id } = code_and_id;
        assert_eq!(
            code_id,
            CodeId::generate(&code),
            "CodeId does not match the provided code"
        );
        Self { code, code_id }
    }

    pub fn into_unchecked(self) -> CodeAndIdUnchecked {
        CodeAndIdUnchecked {
            code: self.code,
            code_id: self.code_id,
        }
    }
}

/// RemoveFromMailbox key; (msgs sources program (mailbox and queue provider), destination user id)
pub type Rfm = (ActorId, ActorId);

/// SendDispatch key; (msgs destinations program (stash and queue provider), message id)
pub type Sd = (ActorId, MessageId);

/// SendUserMessage key; (msgs sources program (mailbox and stash provider))
pub type Sum = ActorId;

/// NOTE: generic keys differs to Vara and have been chosen dependent on storage organization of ethexe.
pub type ScheduledTask = gear_core::tasks::ScheduledTask<Rfm, Sd, Sum>;

/// Scheduler; (block height, scheduled task)
pub type Schedule = BTreeMap<u32, BTreeSet<ScheduledTask>>;
