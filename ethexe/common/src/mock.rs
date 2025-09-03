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

use std::collections::{BTreeSet, VecDeque};

use crate::{
    Address, Announce, AnnounceHash, BlockHeader, CodeBlobInfo, Digest, ProgramStates, Schedule,
    SimpleBlockData,
    db::*,
    events::BlockEvent,
    gear::{BatchCommitment, ChainCommitment, CodeCommitment, Message, StateTransition},
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
pub struct SyncedBlockData {
    pub header: BlockHeader,
    pub events: Vec<BlockEvent>,
    pub validators: NonEmpty<Address>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PreparedBlockData {
    pub codes_queue: VecDeque<CodeId>,
    pub announces: BTreeSet<AnnounceHash>,
    pub last_committed_batch: Digest,
    pub last_committed_announce: AnnounceHash,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BlockData {
    pub hash: H256,
    pub synced: Option<SyncedBlockData>,
    pub prepared: Option<PreparedBlockData>,
}

impl BlockData {
    pub fn as_synced(&self) -> &SyncedBlockData {
        self.synced.as_ref().expect("block not synced")
    }

    pub fn as_prepared(&self) -> &PreparedBlockData {
        self.prepared.as_ref().expect("block not prepared")
    }

    pub fn as_synced_mut(&mut self) -> &mut SyncedBlockData {
        self.synced.as_mut().expect("block not synced")
    }

    pub fn as_prepared_mut(&mut self) -> &mut PreparedBlockData {
        self.prepared.as_mut().expect("block not prepared")
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ComputedAnnounceData {
    pub outcome: Vec<StateTransition>,
    pub program_states: ProgramStates,
    pub schedule: Schedule,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AnnounceData {
    pub announce: Announce,
    pub computed: Option<ComputedAnnounceData>,
}

impl AnnounceData {
    pub fn as_computed(&self) -> &ComputedAnnounceData {
        self.computed.as_ref().expect("announce not computed")
    }

    pub fn as_computed_mut(&mut self) -> &mut ComputedAnnounceData {
        self.computed.as_mut().expect("announce not computed")
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InstrumentedCodeData {
    pub instrumented: InstrumentedCode,
    pub meta: CodeMetadata,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CodeData {
    pub original_bytes: Vec<u8>,
    pub blob_info: CodeBlobInfo,
    pub instrumented: Option<InstrumentedCodeData>,
}

impl CodeData {
    pub fn as_instrumented(&self) -> &InstrumentedCodeData {
        self.instrumented.as_ref().expect("code not instrumented")
    }

    pub fn as_instrumented_mut(&mut self) -> &mut InstrumentedCodeData {
        self.instrumented.as_mut().expect("code not instrumented")
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BlockChain {
    pub blocks: VecDeque<BlockData>,
    pub announces: BTreeMap<AnnounceHash, AnnounceData>,
    pub codes: BTreeMap<CodeId, CodeData>,
}

impl BlockChain {
    pub fn block_top_announce_hash(&self, block_index: usize) -> AnnounceHash {
        self.blocks
            .get(block_index)
            .expect("block index overflow")
            .as_prepared()
            .announces
            .first()
            .copied()
            .expect("no announces found for block")
    }

    pub fn block_top_announce(&self, block_index: usize) -> &AnnounceData {
        self.announces
            .get(&self.block_top_announce_hash(block_index))
            .expect("announce not found")
    }

    pub fn block_top_announce_mut(&mut self, block_index: usize) -> &mut AnnounceData {
        self.announces
            .get_mut(&self.block_top_announce_hash(block_index))
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
            .map(|i| (H256::from_low_u64_be(i as u64), i, i * 12))
            .tuple_windows()
            .map(
                |((parent_hash, _, _), (block_hash, block_height, block_timestamp))| {
                    BlockData {
                        hash: block_hash,
                        synced: Some(SyncedBlockData {
                            header: BlockHeader {
                                height: block_height,
                                timestamp: block_timestamp as u64,
                                parent_hash,
                            },
                            events: Default::default(),
                            validators: validators.clone(),
                        }),
                        prepared: Some(PreparedBlockData {
                            codes_queue: Default::default(),
                            announces: Default::default(), // empty here, filled below with announces
                            last_committed_batch: Digest::zero(),
                            last_committed_announce: AnnounceHash::zero(),
                        }),
                    }
                },
            )
            .collect();

        let mut genesis_announce_hash = None;
        let mut parent_announce_hash = AnnounceHash::zero();
        let announces = blocks
            .iter_mut()
            .map(|block| {
                let announce = Announce::base(block.hash, parent_announce_hash);
                let announce_hash = announce.hash();
                let genesis_announce_hash = genesis_announce_hash.get_or_insert(announce_hash);
                let prepared_data = block.prepared.as_mut().unwrap();
                prepared_data.announces.insert(announce_hash);
                prepared_data.last_committed_announce = *genesis_announce_hash;
                parent_announce_hash = announce_hash;
                (
                    announce_hash,
                    AnnounceData {
                        announce,
                        computed: Some(ComputedAnnounceData {
                            outcome: Default::default(),
                            program_states: Default::default(),
                            schedule: Default::default(),
                        }),
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

        db.set_latest_data(LatestData::default());

        if let Some(genesis) = blocks.front() {
            db.mutate_latest_data(|latest| {
                latest.genesis_block_hash = genesis.hash;
                latest.start_block_hash = genesis.hash;
            })
            .unwrap();

            if let Some(prepared) = &genesis.prepared
                && let Some(first_announce) = prepared.announces.first()
            {
                db.mutate_latest_data(|latest| {
                    latest.genesis_announce_hash = *first_announce;
                    latest.start_announce_hash = *first_announce;
                })
                .unwrap();
            }
        }

        for BlockData {
            hash,
            synced,
            prepared,
        } in blocks
        {
            if let Some(SyncedBlockData {
                header,
                events,
                validators,
            }) = synced
            {
                db.mutate_latest_data(|latest| latest.synced_block_height = header.height)
                    .unwrap();

                db.set_block_header(hash, header);
                db.set_block_events(hash, &events);
                db.set_block_validators(hash, validators);
                db.set_block_synced(hash);
            }

            if let Some(PreparedBlockData {
                codes_queue,
                announces,
                last_committed_batch,
                last_committed_announce,
            }) = prepared
            {
                db.mutate_latest_data(|latest| {
                    latest.prepared_block_hash = hash;
                });

                db.mutate_block_meta(hash, |meta| {
                    *meta = BlockMeta {
                        prepared: true,
                        announces: Some(announces),
                        codes_queue: Some(codes_queue),
                        last_committed_batch: Some(last_committed_batch),
                        last_committed_announce: Some(last_committed_announce),
                    }
                });
            }
        }

        for (announce_hash, AnnounceData { announce, computed }) in announces {
            db.set_announce(announce);
            if let Some(ComputedAnnounceData {
                outcome,
                program_states,
                schedule,
            }) = computed
            {
                db.mutate_latest_data(|latest| {
                    latest.computed_announce_hash = announce_hash;
                });

                db.set_announce_outcome(announce_hash, outcome);
                db.set_announce_program_states(announce_hash, program_states);
                db.set_announce_schedule(announce_hash, schedule);
                db.mutate_announce_meta(announce_hash, |meta| {
                    *meta = AnnounceMeta { computed: true }
                });
            }
        }

        for (
            code_id,
            CodeData {
                original_bytes,
                blob_info,
                instrumented,
            },
        ) in codes
        {
            db.set_original_code(&original_bytes);

            if let Some(InstrumentedCodeData { instrumented, meta }) = instrumented {
                db.set_instrumented_code(1, code_id, instrumented);
                db.set_code_metadata(code_id, meta);
                db.set_code_blob_info(code_id, blob_info);
                db.set_code_valid(code_id, true);
            }
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
