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

use std::collections::VecDeque;

use crate::{
    Address, Announce, AnnounceHash, BlockHeader, CodeBlobInfo, Digest, SimpleBlockData,
    db::*,
    gear::{BatchCommitment, ChainCommitment, CodeCommitment, Message, StateTransition},
    utils,
};
use alloc::{collections::BTreeMap, vec};
use gear_core::code::{CodeMetadata, InstrumentedCode};
use gprimitives::{CodeId, H256};
use itertools::Itertools;
use nonempty::NonEmpty;

pub trait Mock {
    type Args;

    fn mock(args: Self::Args) -> Self;
}

impl Mock for SimpleBlockData {
    type Args = H256;

    fn mock(parent: H256) -> Self {
        SimpleBlockData {
            hash: H256::random(),
            header: BlockHeader {
                height: 43,
                timestamp: 120,
                parent_hash: parent,
            },
        }
    }
}

impl Mock for Announce {
    type Args = (H256, AnnounceHash);

    fn mock((block_hash, parent): (H256, AnnounceHash)) -> Self {
        Announce {
            block_hash,
            parent,
            gas_allowance: Some(100),
            off_chain_transactions: vec![],
        }
    }
}

impl Mock for CodeCommitment {
    type Args = ();

    fn mock(_args: Self::Args) -> Self {
        CodeCommitment {
            id: H256::random().into(),
            valid: true,
        }
    }
}

impl Mock for ChainCommitment {
    type Args = AnnounceHash;

    fn mock(head_announce: Self::Args) -> Self {
        ChainCommitment {
            transitions: vec![StateTransition::mock(()), StateTransition::mock(())],
            head_announce,
        }
    }
}

impl Mock for BatchCommitment {
    type Args = ();

    fn mock(_args: Self::Args) -> Self {
        BatchCommitment {
            block_hash: H256::random(),
            timestamp: 42,
            previous_batch: Digest::random(),
            chain_commitment: Some(ChainCommitment::mock(AnnounceHash::random())),
            code_commitments: vec![CodeCommitment::mock(()), CodeCommitment::mock(())],
            validators_commitment: None,
            rewards_commitment: None,
        }
    }
}

impl Mock for StateTransition {
    type Args = ();

    fn mock(_args: Self::Args) -> Self {
        StateTransition {
            actor_id: H256::random().into(),
            new_state_hash: H256::random(),
            inheritor: H256::random().into(),
            value_to_receive: 123,
            value_claims: vec![],
            messages: vec![Message {
                id: H256::random().into(),
                destination: H256::random().into(),
                payload: b"Hello, World!".to_vec(),
                value: 0,
                reply_details: None,
                call: false,
            }],
            exited: false,
        }
    }
}

pub trait Prepare<DB> {
    type Args;

    fn prepare(self, db: &DB, args: Self::Args) -> Self;
}

impl<DB: AnnounceStorageWrite + BlockMetaStorageWrite + OnChainStorageWrite> Prepare<DB>
    for SimpleBlockData
{
    type Args = AnnounceHash;

    fn prepare(self, db: &DB, last_committed_announce: AnnounceHash) -> Self {
        db.set_block_header(self.hash, self.header);

        let parent_announce = db
            .block_meta(self.header.parent_hash)
            .announces
            .map(|a| *a.first().unwrap())
            .unwrap_or(last_committed_announce);
        let announce = Announce::mock((self.hash, parent_announce));
        let announce_hash = db.set_announce(announce);
        db.set_announce_outcome(announce_hash, Default::default());
        db.mutate_announce_meta(announce_hash, |meta| {
            *meta = AnnounceMeta { computed: true }
        });

        db.mutate_block_meta(self.hash, |meta| {
            *meta = BlockMeta {
                prepared: true,
                announces: Some([announce_hash].into()),
                codes_queue: Some(Default::default()),
                last_committed_batch: None,
                last_committed_announce: Some(last_committed_announce),
            }
        });

        self
    }
}

impl<DB: CodesStorageWrite> Prepare<DB> for CodeCommitment {
    type Args = ();

    fn prepare(self, db: &DB, _args: ()) -> Self {
        db.set_code_valid(self.id, self.valid);
        self
    }
}

impl<DB: AnnounceStorageWrite> Prepare<DB> for ChainCommitment {
    type Args = ();

    fn prepare(self, db: &DB, _args: ()) -> Self {
        let Self {
            transitions,
            head_announce: head,
        } = &self;
        db.set_announce_outcome(*head, transitions.clone());
        self
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FullCodeData {
    pub original_bytes: Vec<u8>,
    pub instrumented: InstrumentedCode,
    pub blob_info: CodeBlobInfo,
    pub meta: CodeMetadata,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BlockChain {
    pub blocks: VecDeque<(H256, FullBlockData)>,
    pub announces: BTreeMap<AnnounceHash, FullAnnounceData>,
    pub codes: BTreeMap<CodeId, FullCodeData>,
}

impl BlockChain {
    pub fn genesis(&self) -> &(H256, FullBlockData) {
        self.blocks.front().expect("empty chain")
    }

    pub fn head(&self) -> &(H256, FullBlockData) {
        self.blocks.back().expect("empty chain")
    }

    pub fn block_top_announce(&self, block_index: usize) -> &FullAnnounceData {
        let block = &self.blocks[block_index];
        let announce_hash = block.1.announces.first().expect("no announces");
        self.announces
            .get(announce_hash)
            .expect("announce not found")
    }

    pub fn block_top_announce_mut(&mut self, block_index: usize) -> &mut FullAnnounceData {
        let block = &self.blocks[block_index];
        let announce_hash = block.1.announces.first().expect("no announces");
        self.announces
            .get_mut(announce_hash)
            .expect("announce not found")
    }
}

impl Mock for BlockChain {
    type Args = (u32, Option<Vec<Address>>);

    fn mock((len, maybe_validators): Self::Args) -> Self {
        let validators =
            NonEmpty::from_vec(maybe_validators.unwrap_or(vec![Address([123; 20])])).unwrap();

        // genesis starts from i == 1
        let mut blocks: VecDeque<_> = (0..len + 1)
            .map(|i| (H256::random(), i, i * 12))
            .tuple_windows()
            .map(
                |((parent_hash, _, _), (block_hash, block_height, block_timestamp))| {
                    let data = FullBlockData {
                        header: BlockHeader {
                            height: block_height,
                            timestamp: block_timestamp as u64,
                            parent_hash,
                        },
                        events: Default::default(),
                        validators: validators.clone(),
                        codes_queue: Default::default(),
                        announces: Default::default(), // empty here, filled below with announces
                        last_committed_batch: Digest::zero(),
                        last_committed_announce: AnnounceHash::zero(),
                    };

                    (block_hash, data)
                },
            )
            .collect();

        let mut genesis_announce_hash = None;
        let mut parent_announce_hash = AnnounceHash::zero();
        let announces = blocks
            .iter_mut()
            .map(|(block_hash, block_data)| {
                let announce = Announce::base(*block_hash, parent_announce_hash);
                let announce_hash = announce.hash();
                let genesis_announce_hash = genesis_announce_hash.get_or_insert(announce_hash);
                block_data.announces.insert(announce_hash);
                block_data.last_committed_announce = *genesis_announce_hash;
                parent_announce_hash = announce_hash;
                (
                    announce_hash,
                    FullAnnounceData {
                        announce,
                        outcome: Default::default(),
                        program_states: Default::default(),
                        schedule: Default::default(),
                    },
                )
            })
            .collect();

        BlockChain {
            blocks,
            announces,
            codes: Default::default(),
        }
    }
}

impl<
    DB: AnnounceStorageWrite
        + BlockMetaStorageWrite
        + OnChainStorageWrite
        + CodesStorageWrite
        + LatestDataStorageWrite,
> Prepare<DB> for BlockChain
{
    type Args = ();

    fn prepare(self, db: &DB, _args: Self::Args) -> Self {
        let BlockChain {
            blocks,
            announces,
            codes,
        } = self.clone();

        for (_, announce_data) in announces {
            utils::setup_announce_in_db(db, announce_data);
        }

        if let Some((
            hash,
            FullBlockData {
                header, validators, ..
            },
        )) = blocks.front().cloned()
        {
            utils::setup_genesis_in_db(db, SimpleBlockData { hash, header }, validators);
        }

        for (block_hash, block_data) in blocks {
            utils::setup_block_in_db(db, block_hash, block_data);
        }

        for (
            code_id,
            FullCodeData {
                original_bytes,
                instrumented,
                blob_info,
                meta,
            },
        ) in codes
        {
            db.set_original_code(&original_bytes);
            db.set_instrumented_code(1, code_id, instrumented);
            db.set_code_metadata(code_id, meta);
            db.set_code_blob_info(code_id, blob_info);
            db.set_code_valid(code_id, true);
        }

        self
    }
}

pub trait DBMockExt {
    fn simple_block_data(&self, block: H256) -> SimpleBlockData;
    fn top_announce_hash(&self, block: H256) -> AnnounceHash;
}

impl<DB: OnChainStorageRead + BlockMetaStorageRead> DBMockExt for DB {
    fn simple_block_data(&self, block: H256) -> SimpleBlockData {
        let header = self.block_header(block).expect("block header not found");
        SimpleBlockData {
            hash: block,
            header,
        }
    }

    fn top_announce_hash(&self, block: H256) -> AnnounceHash {
        self.block_meta(block)
            .announces
            .expect("block announces not found")
            .into_iter()
            .next()
            .expect("must be at list one announce")
    }
}
