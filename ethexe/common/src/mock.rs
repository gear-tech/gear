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

use crate::{
    Address, Announce, BlockData, BlockHeader, CodeBlobInfo, ComputedAnnounce, Digest, HashOf,
    ProgramStates, ProtocolTimelines, Schedule, SimpleBlockData, ValidatorsVec,
    consensus::BatchCommitmentValidationRequest,
    db::*,
    ecdsa::{PrivateKey, SignedMessage},
    events::BlockEvent,
    gear::{BatchCommitment, ChainCommitment, CodeCommitment, Message, StateTransition},
    injected::{AddressedInjectedTransaction, InjectedTransaction},
};
use alloc::{collections::BTreeMap, vec};
use gear_core::{
    code::{CodeMetadata, InstrumentedCode},
    limited::LimitedVec,
};
use gprimitives::{CodeId, H256};
use itertools::Itertools;
use std::collections::{BTreeSet, VecDeque};
pub use tap::Tap;

// TODO #4881: use `proptest::Arbitrary` instead
pub trait Mock<Args = ()> {
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

impl Mock<()> for ProtocolTimelines {
    fn mock(_args: ()) -> Self {
        Self {
            genesis_ts: 0,
            era: 1000,
            election: 200,
            slot: 10,
        }
    }
}

impl Mock<(H256, HashOf<Announce>)> for Announce {
    fn mock((block_hash, parent): (H256, HashOf<Announce>)) -> Self {
        Announce {
            block_hash,
            parent,
            gas_allowance: Some(100),
            injected_transactions: vec![],
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
            expiry: 10,
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
            value_to_receive_negative_sign: false,
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

impl Mock<()> for InjectedTransaction {
    fn mock((): ()) -> Self {
        Self {
            destination: Default::default(),
            payload: LimitedVec::new(),
            value: 0,
            reference_block: Default::default(),
            salt: LimitedVec::try_from(H256::random().as_bytes())
                .expect("`H256` is small enough for a salt"),
        }
    }
}

impl Mock<PrivateKey> for AddressedInjectedTransaction {
    fn mock(pk: PrivateKey) -> Self {
        AddressedInjectedTransaction {
            recipient: Default::default(),
            tx: SignedMessage::create(pk, InjectedTransaction::mock(()))
                .expect("Signing injected transaction will succeed"),
        }
    }
}

impl Mock<()> for AddressedInjectedTransaction {
    fn mock(_args: ()) -> Self {
        AddressedInjectedTransaction::mock(PrivateKey::random())
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SyncedBlockData {
    pub header: BlockHeader,
    pub events: Vec<BlockEvent>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PreparedBlockData {
    pub codes_queue: VecDeque<CodeId>,
    pub announces: Option<BTreeSet<HashOf<Announce>>>,
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
    #[track_caller]
    pub fn as_synced(&self) -> &SyncedBlockData {
        self.synced.as_ref().expect("block not synced")
    }

    #[track_caller]
    pub fn as_prepared(&self) -> &PreparedBlockData {
        self.prepared.as_ref().expect("block not prepared")
    }

    #[track_caller]
    pub fn as_synced_mut(&mut self) -> &mut SyncedBlockData {
        self.synced.as_mut().expect("block not synced")
    }

    #[track_caller]
    pub fn as_prepared_mut(&mut self) -> &mut PreparedBlockData {
        self.prepared.as_mut().expect("block not prepared")
    }

    #[track_caller]
    pub fn to_simple(&self) -> SimpleBlockData {
        SimpleBlockData {
            hash: self.hash,
            header: self.as_synced().header,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct MockComputedAnnounceData {
    pub outcome: Vec<StateTransition>,
    pub program_states: ProgramStates,
    pub schedule: Schedule,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AnnounceData {
    pub announce: Announce,
    pub computed: Option<MockComputedAnnounceData>,
}

impl AnnounceData {
    pub fn as_computed(&self) -> &MockComputedAnnounceData {
        self.computed.as_ref().expect("announce not computed")
    }

    pub fn as_computed_mut(&mut self) -> &mut MockComputedAnnounceData {
        self.computed.as_mut().expect("announce not computed")
    }

    pub fn setup(self, db: &impl AnnounceStorageRW) -> Self {
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
    pub validators: ValidatorsVec,
    pub config: DBConfig,
    pub globals: DBGlobals,
}

impl BlockChain {
    #[track_caller]
    pub fn block_top_announce_hash(&self, block_index: usize) -> HashOf<Announce> {
        self.blocks
            .get(block_index)
            .expect("block index overflow")
            .as_prepared()
            .announces
            .iter()
            .flatten()
            .next()
            .copied()
            .expect("no announces found for block")
    }

    #[track_caller]
    pub fn block_top_announce(&self, block_index: usize) -> &AnnounceData {
        self.announces
            .get(&self.block_top_announce_hash(block_index))
            .expect("announce not found")
    }

    #[track_caller]
    pub fn block_top_announce_mut(&mut self, block_index: usize) -> &mut AnnounceData {
        self.announces
            .get_mut(&self.block_top_announce_hash(block_index))
            .expect("announce not found")
    }

    #[track_caller]
    pub fn setup<DB>(self, db: &DB) -> Self
    where
        DB: AnnounceStorageRW
            + BlockMetaStorageRW
            + OnChainStorageRW
            + CodesStorageRW
            + SetConfig
            + SetGlobals,
    {
        let BlockChain {
            blocks,
            announces,
            codes,
            validators,
            config,
            globals,
        } = self.clone();

        db.set_config(config.clone());
        db.set_globals(globals);

        for BlockFullData {
            hash,
            synced,
            prepared,
        } in blocks
        {
            if let Some(SyncedBlockData { header, events }) = synced {
                db.set_block_header(hash, header);
                db.set_block_events(hash, &events);
                db.set_block_synced(hash);

                let block_era = config.timelines.era_from_ts(header.timestamp);
                db.set_validators(block_era, validators.clone());
                db.set_block_validators_committed_for_era(hash, block_era);
            }

            if let Some(PreparedBlockData {
                codes_queue,
                announces,
                last_committed_batch,
                last_committed_announce,
            }) = prepared
            {
                db.mutate_block_meta(hash, |meta| {
                    *meta = BlockMeta {
                        prepared: true,
                        announces,
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

impl Mock<(u32, ValidatorsVec)> for BlockChain {
    /// `len` - length of chain not counting genesis block
    fn mock((len, validators): (u32, ValidatorsVec)) -> Self {
        let slot = 10;
        let genesis_height = 1_000_000;
        let genesis_ts = 1_000_000;

        // i = 0, h = None - genesis parent
        // i = 1, h = 0 - genesis
        // i = 2, h = 1 - first block
        // ...
        // i = len + 1, h = len - last block
        let mut blocks: VecDeque<_> = (0..len + 2)
            .map(|i| {
                if let Some(h) = i.checked_sub(1) {
                    // Human readable blocks, to avoid zero values append some readable numbers
                    let hash = H256::from_low_u64_be(h as u64).tap_mut(|hash| hash.0[0] = 0x10);
                    let height = genesis_height + h;
                    let timestamp = genesis_ts + h * slot as u32;
                    (hash, height, timestamp)
                } else {
                    (H256([u8::MAX; 32]), 0, 0)
                }
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
                        }),
                        prepared: Some(PreparedBlockData {
                            codes_queue: Default::default(),
                            announces: Some(Default::default()), // empty here, filled below with announces
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
                prepared_data
                    .announces
                    .as_mut()
                    .unwrap()
                    .insert(announce_hash);
                prepared_data.last_committed_announce = *genesis_announce_hash;
                parent_announce_hash = announce_hash;
                (
                    announce_hash,
                    AnnounceData {
                        announce,
                        computed: Some(MockComputedAnnounceData {
                            outcome: Default::default(),
                            program_states: Default::default(),
                            schedule: Default::default(),
                        }),
                    },
                )
            })
            .collect();

        let config = DBConfig {
            version: 0,
            chain_id: 0,
            router_address: Address(<[u8; 20]>::try_from(&H256::random()[..20]).unwrap()),
            timelines: ProtocolTimelines {
                genesis_ts: genesis_ts as u64,
                era: slot * 100,
                election: slot * 20,
                slot,
            },
            genesis_block_hash: blocks[0].hash,
            genesis_announce_hash: genesis_announce_hash.unwrap(),
        };

        let globals = DBGlobals {
            start_block_hash: blocks[0].hash,
            start_announce_hash: genesis_announce_hash.unwrap(),
            latest_synced_block: blocks.back().unwrap().to_simple(),
            latest_prepared_block_hash: blocks.back().unwrap().hash,
            latest_computed_announce_hash: parent_announce_hash,
        };

        BlockChain {
            blocks,
            announces,
            codes: Default::default(),
            validators,
            config,
            globals,
        }
    }
}

impl Mock<u32> for BlockChain {
    /// `len` - length of chain not counting genesis block
    fn mock(len: u32) -> Self {
        BlockChain::mock((len, Default::default()))
    }
}

pub trait DBMockExt {
    fn simple_block_data(&self, block: H256) -> SimpleBlockData;
    fn top_announce_hash(&self, block: H256) -> HashOf<Announce>;
}

impl<DB: OnChainStorageRO + BlockMetaStorageRO> DBMockExt for DB {
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
        DB: OnChainStorageRW,
    {
        db.set_block_header(self.hash, self.header);
        db.set_block_events(self.hash, &[]);
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
        DB: OnChainStorageRW,
    {
        db.set_block_header(self.hash, self.header);
        db.set_block_events(self.hash, &self.events);
        db.set_block_synced(self.hash);
        self
    }
}

impl Mock<()> for DBConfig {
    fn mock(_args: ()) -> Self {
        DBConfig {
            version: 0,
            chain_id: 0,
            router_address: Address::default(),
            timelines: ProtocolTimelines::mock(()),
            genesis_block_hash: H256::random(),
            genesis_announce_hash: HashOf::random(),
        }
    }
}

impl Mock for ComputedAnnounce {
    fn mock(_: ()) -> Self {
        Self {
            announce_hash: HashOf::random(),
            promises: Default::default(),
        }
    }
}

impl Mock<HashOf<Announce>> for ComputedAnnounce {
    fn mock(announce_hash: HashOf<Announce>) -> Self {
        Self {
            announce_hash,
            promises: Default::default(),
        }
    }
}

impl Mock<()> for DBGlobals {
    fn mock(_args: ()) -> Self {
        DBGlobals {
            start_block_hash: H256::random(),
            start_announce_hash: HashOf::random(),
            latest_synced_block: SimpleBlockData::mock(()),
            latest_prepared_block_hash: H256::random(),
            latest_computed_announce_hash: HashOf::random(),
        }
    }
}
