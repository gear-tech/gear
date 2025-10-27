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

pub use tap::Tap;

use crate::{
    Address, Announce, BlockData, BlockHeader, CodeBlobInfo, Digest, HashOf, ProgramStates,
    Schedule, SimpleBlockData,
    consensus::BatchCommitmentValidationRequest,
    db::*,
    events::BlockEvent,
    gear::{BatchCommitment, ChainCommitment, CodeCommitment, Message, StateTransition},
    network::ValidatorMessage,
};
use alloc::{collections::BTreeMap, vec};
use gear_core::code::{CodeMetadata, InstrumentedCode};
use gprimitives::{CodeId, H256};
use itertools::Itertools;
use nonempty::{NonEmpty, nonempty};
use std::collections::{BTreeSet, VecDeque};

// TODO #4881: use `proptest::Arbitrary` instead
pub trait Mock<Args> {
    fn mock(args: Args) -> Self;
}

impl Mock<H256> for SimpleBlockData {
    fn mock(parent_hash: H256) -> Self {
        SimpleBlockData {
            hash: H256::random(),
            header: BlockHeader {
                height: 43,
                timestamp: 120,
                parent_hash,
            },
        }
    }
}

impl Mock<()> for SimpleBlockData {
    fn mock(_args: ()) -> Self {
        SimpleBlockData::mock(H256::random())
    }
}

impl Mock<(H256, HashOf<Announce>)> for Announce {
    fn mock((block_hash, parent): (H256, HashOf<Announce>)) -> Self {
        Announce {
            block_hash,
            parent,
            gas_allowance: Some(100),
            off_chain_transactions: vec![],
        }
    }
}

impl Mock<H256> for Announce {
    fn mock(block_hash: H256) -> Self {
        Announce::mock((block_hash, HashOf::random()))
    }
}

impl Mock<()> for Announce {
    fn mock(_args: ()) -> Self {
        Announce::mock(H256::random())
    }
}

impl Mock<()> for CodeCommitment {
    fn mock(_args: ()) -> Self {
        CodeCommitment {
            id: H256::random().into(),
            valid: true,
        }
    }
}

impl Mock<HashOf<Announce>> for ChainCommitment {
    fn mock(head_announce: HashOf<Announce>) -> Self {
        ChainCommitment {
            transitions: vec![StateTransition::mock(()), StateTransition::mock(())],
            head_announce,
        }
    }
}

impl Mock<()> for ChainCommitment {
    fn mock(_args: ()) -> Self {
        ChainCommitment::mock(HashOf::random())
    }
}

impl Mock<()> for BatchCommitment {
    fn mock(_args: ()) -> Self {
        BatchCommitment {
            block_hash: H256::random(),
            timestamp: 42,
            previous_batch: Digest::random(),
            chain_commitment: Some(ChainCommitment::mock(HashOf::random())),
            code_commitments: vec![CodeCommitment::mock(()), CodeCommitment::mock(())],
            validators_commitment: None,
            rewards_commitment: None,
        }
    }
}

impl Mock<()> for BatchCommitmentValidationRequest {
    fn mock(_args: ()) -> Self {
        BatchCommitmentValidationRequest {
            digest: H256::random().0.into(),
            head: Some(HashOf::random()),
            codes: vec![CodeCommitment::mock(()).id, CodeCommitment::mock(()).id],
            validators: false,
            rewards: false,
        }
    }
}

impl Mock<()> for StateTransition {
    fn mock(_args: ()) -> Self {
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

impl<T: Mock<()>> Mock<()> for ValidatorMessage<T> {
    fn mock(_args: ()) -> Self {
        Self {
            block: H256::random(),
            payload: T::mock(()),
        }
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
    pub announces: BTreeSet<HashOf<Announce>>,
    pub last_committed_batch: Digest,
    pub last_committed_announce: HashOf<Announce>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BlockFullData {
    pub hash: H256,
    pub synced: Option<SyncedBlockData>,
    pub prepared: Option<PreparedBlockData>,
}

impl BlockFullData {
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

    pub fn to_simple(&self) -> SimpleBlockData {
        SimpleBlockData {
            hash: self.hash,
            header: self.as_synced().header,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
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

    pub fn setup(self, db: &impl AnnounceStorageWrite) -> Self {
        let announce_hash = db.set_announce(self.announce.clone());

        if let Some(computed) = &self.computed {
            db.set_announce_outcome(announce_hash, computed.outcome.clone());
            db.set_announce_program_states(announce_hash, computed.program_states.clone());
            db.set_announce_schedule(announce_hash, computed.schedule.clone());
            db.mutate_announce_meta(announce_hash, |meta| {
                *meta = AnnounceMeta { computed: true }
            });
        }

        self
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
    pub blocks: VecDeque<BlockFullData>,
    pub announces: BTreeMap<HashOf<Announce>, AnnounceData>,
    pub codes: BTreeMap<CodeId, CodeData>,
}

impl BlockChain {
    pub fn block_top_announce_hash(&self, block_index: usize) -> HashOf<Announce> {
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

    pub fn setup<DB>(self, db: &DB) -> Self
    where
        DB: AnnounceStorageWrite
            + BlockMetaStorageWrite
            + OnChainStorageWrite
            + CodesStorageWrite
            + LatestDataStorageWrite,
    {
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

        for BlockFullData {
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

                if let Some(announce_hash) = announces.last().copied() {
                    db.mutate_latest_data(|latest| {
                        latest.computed_announce_hash = announce_hash;
                    });
                }

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

        announces.into_iter().for_each(|(_, data)| {
            let _ = data.setup(db);
        });

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

impl Mock<(u32, NonEmpty<Address>)> for BlockChain {
    /// `len` - length of chain not counting genesis block
    fn mock((len, validators): (u32, NonEmpty<Address>)) -> Self {
        // i = 0 - genesis parent
        // i = 1 - genesis
        // i = 2 - first block
        // ...
        // i = len + 1 - last block
        let mut blocks: VecDeque<_> = (0..len + 2)
            .map(|i| {
                // Human readable blocks, to avoid zero values append some readable numbers
                i.checked_sub(1)
                    .map(|h| {
                        (
                            H256::from_low_u64_be(0x1_000_000 + h as u64),
                            1_000_000 + h,
                            1_000_000 + h * 10,
                        )
                    })
                    .unwrap_or((H256([u8::MAX; 32]), 0, 0))
            })
            .tuple_windows()
            .map(
                |((parent_hash, _, _), (block_hash, block_height, block_timestamp))| {
                    BlockFullData {
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
                            last_committed_announce: HashOf::zero(),
                        }),
                    }
                },
            )
            .collect();

        let mut genesis_announce_hash = None;
        let mut parent_announce_hash = HashOf::zero();
        let announces = blocks
            .iter_mut()
            .map(|block| {
                let announce = Announce::base(block.hash, parent_announce_hash);
                let announce_hash = announce.to_hash();
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

impl Mock<u32> for BlockChain {
    /// `len` - length of chain not counting genesis block
    fn mock(len: u32) -> Self {
        BlockChain::mock((len, nonempty![Address([123; 20])]))
    }
}

pub trait DBMockExt {
    fn simple_block_data(&self, block: H256) -> SimpleBlockData;
    fn top_announce_hash(&self, block: H256) -> HashOf<Announce>;
}

impl<DB: OnChainStorageRead + BlockMetaStorageRead> DBMockExt for DB {
    #[track_caller]
    fn simple_block_data(&self, block: H256) -> SimpleBlockData {
        let header = self.block_header(block).expect("block header not found");
        SimpleBlockData {
            hash: block,
            header,
        }
    }

    #[track_caller]
    fn top_announce_hash(&self, block: H256) -> HashOf<Announce> {
        self.block_meta(block)
            .announces
            .expect("block announces not found")
            .into_iter()
            .next()
            .expect("must be at list one announce")
    }
}

impl SimpleBlockData {
    pub fn setup<DB>(self, db: &DB) -> Self
    where
        DB: OnChainStorageWrite,
    {
        db.set_block_header(self.hash, self.header);
        db.set_block_events(self.hash, &[]);
        db.set_block_validators(self.hash, nonempty![Address([123; 20])]);
        db.set_block_synced(self.hash);
        self
    }

    pub fn next_block(self) -> Self {
        Self {
            hash: H256::from_low_u64_be(self.hash.to_low_u64_be() + 1),
            header: BlockHeader {
                height: self.header.height + 1,
                parent_hash: self.hash,
                timestamp: self.header.timestamp + 10,
            },
        }
    }
}

impl BlockData {
    pub fn setup<DB>(self, db: &DB) -> Self
    where
        DB: OnChainStorageWrite,
    {
        db.set_block_header(self.hash, self.header);
        db.set_block_events(self.hash, &self.events);
        db.set_block_validators(self.hash, nonempty![Address([123; 20])]);
        db.set_block_synced(self.hash);
        self
    }
}
