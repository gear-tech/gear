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
    Address, Announce, BlockData, BlockHeader, CodeBlobInfo, Digest, HashOf, ProgramStates,
    ProtocolTimelines, Schedule, SimpleBlockData, ValidatorsVec,
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
use gprimitives::{ActorId, CodeId, H256, MessageId};
use itertools::Itertools;
use proptest::{
    arbitrary::Arbitrary,
    collection,
    prelude::{BoxedStrategy, Strategy, any},
    strategy::{Just, ValueTree},
    test_runner::TestRunner,
};
use std::collections::{BTreeSet, VecDeque};
pub use tap::Tap;

fn arbitrary_value<T>(args: T::Parameters) -> T
where
    T: Arbitrary + 'static,
{
    T::arbitrary_with(args)
        .new_tree(&mut TestRunner::default())
        .expect("mock strategy must produce a value")
        .current()
}

pub trait Mock<Args = ()> {
    fn mock(args: Args) -> Self;
}

impl<T, Args> Mock<Args> for T
where
    T: Arbitrary + 'static,
    Args: Into<T::Parameters>,
{
    fn mock(args: Args) -> Self {
        arbitrary_value::<T>(args.into())
    }
}

#[derive(Debug, Clone, Copy, Default)]
pub struct BlockHeaderParams {
    parent_hash: Option<H256>,
}

impl From<()> for BlockHeaderParams {
    fn from((): ()) -> Self {
        Self::default()
    }
}

impl From<H256> for BlockHeaderParams {
    fn from(parent_hash: H256) -> Self {
        Self {
            parent_hash: Some(parent_hash),
        }
    }
}

#[derive(Debug, Clone, Copy, Default)]
pub struct AnnounceParams {
    block_hash: Option<H256>,
    parent: Option<HashOf<Announce>>,
}

impl From<()> for AnnounceParams {
    fn from((): ()) -> Self {
        Self::default()
    }
}

impl From<H256> for AnnounceParams {
    fn from(block_hash: H256) -> Self {
        Self {
            block_hash: Some(block_hash),
            parent: None,
        }
    }
}

impl From<(H256, HashOf<Announce>)> for AnnounceParams {
    fn from((block_hash, parent): (H256, HashOf<Announce>)) -> Self {
        Self {
            block_hash: Some(block_hash),
            parent: Some(parent),
        }
    }
}

#[derive(Debug, Clone, Copy, Default)]
pub struct ChainCommitmentParams {
    head_announce: Option<HashOf<Announce>>,
}

impl From<()> for ChainCommitmentParams {
    fn from((): ()) -> Self {
        Self::default()
    }
}

impl From<HashOf<Announce>> for ChainCommitmentParams {
    fn from(head_announce: HashOf<Announce>) -> Self {
        Self {
            head_announce: Some(head_announce),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct BlockChainParams {
    len: u32,
    validators: ValidatorsVec,
}

impl From<u32> for BlockChainParams {
    fn from(len: u32) -> Self {
        Self {
            len,
            validators: Default::default(),
        }
    }
}

impl From<(u32, ValidatorsVec)> for BlockChainParams {
    fn from((len, validators): (u32, ValidatorsVec)) -> Self {
        Self { len, validators }
    }
}

#[derive(Debug, Clone, Default)]
pub struct AddressedInjectedTransactionParams {
    signer: Option<PrivateKey>,
}

impl From<()> for AddressedInjectedTransactionParams {
    fn from((): ()) -> Self {
        Self::default()
    }
}

impl From<PrivateKey> for AddressedInjectedTransactionParams {
    fn from(signer: PrivateKey) -> Self {
        Self {
            signer: Some(signer),
        }
    }
}

fn h256_strategy() -> BoxedStrategy<H256> {
    any::<[u8; 32]>().prop_map(Into::into).boxed()
}

fn digest_strategy() -> BoxedStrategy<Digest> {
    any::<[u8; 32]>().prop_map(Digest).boxed()
}

fn address_strategy() -> BoxedStrategy<Address> {
    any::<[u8; 20]>().prop_map(Address).boxed()
}

fn actor_id_strategy() -> BoxedStrategy<ActorId> {
    h256_strategy().prop_map(Into::into).boxed()
}

fn code_id_strategy() -> BoxedStrategy<CodeId> {
    h256_strategy().prop_map(Into::into).boxed()
}

fn message_id_strategy() -> BoxedStrategy<MessageId> {
    h256_strategy().prop_map(Into::into).boxed()
}

fn hash_of_strategy<T: 'static>() -> BoxedStrategy<HashOf<T>> {
    h256_strategy()
        .prop_map(|hash| unsafe { HashOf::new(hash) })
        .boxed()
}

fn private_key_strategy() -> BoxedStrategy<PrivateKey> {
    any::<[u8; 32]>()
        .prop_filter_map("valid secp256k1 private key", |seed| {
            PrivateKey::from_seed(seed).ok()
        })
        .boxed()
}

fn limited_bytes_strategy<const N: usize>(
    range: impl Into<collection::SizeRange>,
) -> BoxedStrategy<LimitedVec<u8, N>> {
    collection::vec(any::<u8>(), range)
        .prop_map(|bytes| {
            LimitedVec::try_from(bytes).expect("strategy range must fit within LimitedVec bound")
        })
        .boxed()
}

impl Arbitrary for SimpleBlockData {
    type Parameters = BlockHeaderParams;
    type Strategy = BoxedStrategy<Self>;

    fn arbitrary_with(args: Self::Parameters) -> Self::Strategy {
        (h256_strategy(), BlockHeader::arbitrary_with(args))
            .prop_map(|(hash, header)| Self { hash, header })
            .boxed()
    }
}

impl Arbitrary for BlockHeader {
    type Parameters = BlockHeaderParams;
    type Strategy = BoxedStrategy<Self>;

    fn arbitrary_with(args: Self::Parameters) -> Self::Strategy {
        let parent_hash = match args.parent_hash {
            Some(parent_hash) => Just(parent_hash).boxed(),
            None => h256_strategy(),
        };

        parent_hash
            .prop_map(|parent_hash| Self {
                height: 43,
                timestamp: 120,
                parent_hash,
            })
            .boxed()
    }
}

impl Arbitrary for ProtocolTimelines {
    type Parameters = ();
    type Strategy = BoxedStrategy<Self>;

    fn arbitrary_with(_args: Self::Parameters) -> Self::Strategy {
        Just(Self {
            genesis_ts: 0,
            era: 1000,
            election: 200,
            slot: 10,
        })
        .boxed()
    }
}

impl Arbitrary for Announce {
    type Parameters = AnnounceParams;
    type Strategy = BoxedStrategy<Self>;

    fn arbitrary_with(args: Self::Parameters) -> Self::Strategy {
        let block_hash = match args.block_hash {
            Some(block_hash) => Just(block_hash).boxed(),
            None => h256_strategy(),
        };
        let parent = match args.parent {
            Some(parent) => Just(parent).boxed(),
            None => hash_of_strategy(),
        };

        (block_hash, parent)
            .prop_map(|(block_hash, parent)| Self {
                block_hash,
                parent,
                gas_allowance: Some(100),
                injected_transactions: vec![],
            })
            .boxed()
    }
}

impl Arbitrary for CodeCommitment {
    type Parameters = ();
    type Strategy = BoxedStrategy<Self>;

    fn arbitrary_with(_args: Self::Parameters) -> Self::Strategy {
        code_id_strategy()
            .prop_map(|id| Self { id, valid: true })
            .boxed()
    }
}

impl Arbitrary for ChainCommitment {
    type Parameters = ChainCommitmentParams;
    type Strategy = BoxedStrategy<Self>;

    fn arbitrary_with(args: Self::Parameters) -> Self::Strategy {
        let head_announce = match args.head_announce {
            Some(head_announce) => Just(head_announce).boxed(),
            None => hash_of_strategy(),
        };

        (
            StateTransition::arbitrary_with(()),
            StateTransition::arbitrary_with(()),
            head_announce,
        )
            .prop_map(|(first, second, head_announce)| Self {
                transitions: vec![first, second],
                head_announce,
            })
            .boxed()
    }
}

impl Arbitrary for BatchCommitment {
    type Parameters = ();
    type Strategy = BoxedStrategy<Self>;

    fn arbitrary_with(_args: Self::Parameters) -> Self::Strategy {
        (
            h256_strategy(),
            digest_strategy(),
            ChainCommitment::arbitrary_with(().into()),
            CodeCommitment::arbitrary_with(()),
            CodeCommitment::arbitrary_with(()),
        )
            .prop_map(
                |(
                    block_hash,
                    previous_batch,
                    chain_commitment,
                    code_commitment_1,
                    code_commitment_2,
                )| Self {
                    block_hash,
                    timestamp: 42,
                    previous_batch,
                    expiry: 10,
                    chain_commitment: Some(chain_commitment),
                    code_commitments: vec![code_commitment_1, code_commitment_2],
                    validators_commitment: None,
                    rewards_commitment: None,
                },
            )
            .boxed()
    }
}

impl Arbitrary for BatchCommitmentValidationRequest {
    type Parameters = ();
    type Strategy = BoxedStrategy<Self>;

    fn arbitrary_with(_args: Self::Parameters) -> Self::Strategy {
        (
            digest_strategy(),
            hash_of_strategy::<Announce>(),
            code_id_strategy(),
            code_id_strategy(),
        )
            .prop_map(|(digest, head, code_1, code_2)| Self {
                digest,
                head: Some(head),
                codes: vec![code_1, code_2],
                validators: false,
                rewards: false,
            })
            .boxed()
    }
}

impl Arbitrary for StateTransition {
    type Parameters = ();
    type Strategy = BoxedStrategy<Self>;

    fn arbitrary_with(_args: Self::Parameters) -> Self::Strategy {
        (
            actor_id_strategy(),
            h256_strategy(),
            actor_id_strategy(),
            message_id_strategy(),
            actor_id_strategy(),
        )
            .prop_map(
                |(actor_id, new_state_hash, inheritor, message_id, destination)| Self {
                    actor_id,
                    new_state_hash,
                    exited: false,
                    inheritor,
                    value_to_receive: 123,
                    value_to_receive_negative_sign: false,
                    value_claims: vec![],
                    messages: vec![Message {
                        id: message_id,
                        destination,
                        payload: b"Hello, World!".to_vec(),
                        value: 0,
                        reply_details: None,
                        call: false,
                    }],
                },
            )
            .boxed()
    }
}

impl Arbitrary for InjectedTransaction {
    type Parameters = ();
    type Strategy = BoxedStrategy<Self>;

    fn arbitrary_with(_args: Self::Parameters) -> Self::Strategy {
        limited_bytes_strategy::<32>(32..=32)
            .prop_map(|salt| Self {
                destination: Default::default(),
                payload: LimitedVec::new(),
                value: 0,
                reference_block: Default::default(),
                salt,
            })
            .boxed()
    }
}

impl Arbitrary for AddressedInjectedTransaction {
    type Parameters = AddressedInjectedTransactionParams;
    type Strategy = BoxedStrategy<Self>;

    fn arbitrary_with(args: Self::Parameters) -> Self::Strategy {
        let signer = match args.signer {
            Some(signer) => Just(signer).boxed(),
            None => private_key_strategy(),
        };

        (
            address_strategy(),
            signer,
            InjectedTransaction::arbitrary_with(()),
        )
            .prop_map(|(recipient, signer, tx)| Self {
                recipient,
                tx: SignedMessage::create(signer, tx)
                    .expect("signing injected transaction must succeed"),
            })
            .boxed()
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
    pub synced: SyncedBlockData,
    pub prepared: Option<PreparedBlockData>,
}

impl BlockFullData {
    #[track_caller]
    pub fn assert_prepared(&self) -> &PreparedBlockData {
        self.prepared.as_ref().expect("block is not prepared")
    }

    #[track_caller]
    pub fn assert_prepared_mut(&mut self) -> &mut PreparedBlockData {
        self.prepared.as_mut().expect("block is not prepared")
    }

    #[track_caller]
    pub fn to_simple(&self) -> SimpleBlockData {
        SimpleBlockData {
            hash: self.hash,
            header: self.synced.header,
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
            .assert_prepared()
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
    pub fn block_top_announce_mutate(
        &mut self,
        block_index: usize,
        f: impl FnOnce(&mut AnnounceData),
    ) -> HashOf<Announce> {
        let announce_hash = self.block_top_announce_hash(block_index);
        let mut announce_data = self
            .announces
            .remove(&announce_hash)
            .expect("Announce not found");
        f(&mut announce_data);

        self.blocks[block_index]
            .prepared
            .as_mut()
            .expect("block not prepared")
            .announces
            .as_mut()
            .expect("block announces not found")
            .remove(&announce_hash);

        let new_announce_hash = announce_data.announce.to_hash();
        self.announces.insert(new_announce_hash, announce_data);

        self.blocks[block_index]
            .assert_prepared_mut()
            .announces
            .as_mut()
            .expect("block announces not found")
            .insert(new_announce_hash);

        new_announce_hash
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
            synced: SyncedBlockData { header, events },
            prepared,
        } in blocks
        {
            db.set_block_header(hash, header);
            db.set_block_events(hash, &events);
            db.set_block_synced(hash);

            let block_era = config.timelines.era_from_ts(header.timestamp);
            db.set_validators(block_era, validators.clone());

            if let Some(PreparedBlockData {
                codes_queue,
                announces,
                last_committed_batch,
                last_committed_announce,
            }) = prepared
            {
                if let Some(announces) = announces {
                    db.set_block_announces(hash, announces);
                }

                db.mutate_block_meta(hash, |meta| {
                    *meta = BlockMeta {
                        prepared: true,
                        codes_queue: Some(codes_queue),
                        last_committed_batch: Some(last_committed_batch),
                        last_committed_announce: Some(last_committed_announce),
                        latest_era_validators_committed: block_era,
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

    fn with_params(params: BlockChainParams, router_address: Address) -> Self {
        let BlockChainParams { len, validators } = params;
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
                        synced: SyncedBlockData {
                            header: BlockHeader {
                                height: block_height,
                                timestamp: block_timestamp as u64,
                                parent_hash,
                            },
                            events: Default::default(),
                        },
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
            router_address,
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

        Self {
            blocks,
            announces,
            codes: Default::default(),
            validators,
            config,
            globals,
        }
    }
}

impl Arbitrary for BlockChain {
    type Parameters = BlockChainParams;
    type Strategy = BoxedStrategy<Self>;

    fn arbitrary_with(args: Self::Parameters) -> Self::Strategy {
        address_strategy()
            .prop_map(move |router_address| Self::with_params(args.clone(), router_address))
            .boxed()
    }
}

pub trait DBMockExt {
    fn simple_block_data(&self, block: H256) -> SimpleBlockData;
    fn top_announce_hash(&self, block: H256) -> HashOf<Announce>;
}

impl<DB: OnChainStorageRO + BlockMetaStorageRO + AnnounceStorageRO> DBMockExt for DB {
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
        self.block_announces(block)
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

impl Arbitrary for DBConfig {
    type Parameters = ();
    type Strategy = BoxedStrategy<Self>;

    fn arbitrary_with(_args: Self::Parameters) -> Self::Strategy {
        (
            ProtocolTimelines::arbitrary_with(()),
            h256_strategy(),
            hash_of_strategy::<Announce>(),
        )
            .prop_map(
                |(timelines, genesis_block_hash, genesis_announce_hash)| Self {
                    version: 0,
                    chain_id: 0,
                    router_address: Address::default(),
                    timelines,
                    genesis_block_hash,
                    genesis_announce_hash,
                },
            )
            .boxed()
    }
}

impl Arbitrary for DBGlobals {
    type Parameters = ();
    type Strategy = BoxedStrategy<Self>;

    fn arbitrary_with(_args: Self::Parameters) -> Self::Strategy {
        (
            h256_strategy(),
            hash_of_strategy::<Announce>(),
            SimpleBlockData::arbitrary_with(().into()),
            h256_strategy(),
            hash_of_strategy::<Announce>(),
        )
            .prop_map(
                |(
                    start_block_hash,
                    start_announce_hash,
                    latest_synced_block,
                    latest_prepared_block_hash,
                    latest_computed_announce_hash,
                )| Self {
                    start_block_hash,
                    start_announce_hash,
                    latest_synced_block,
                    latest_prepared_block_hash,
                    latest_computed_announce_hash,
                },
            )
            .boxed()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn addressed_injected_transaction_mock_produces_distinct_hashes() {
        let tx_hashes: std::collections::BTreeSet<_> = (0..8)
            .map(|_| AddressedInjectedTransaction::mock(()).tx.data().to_hash())
            .collect();

        assert_eq!(tx_hashes.len(), 8);
    }
}
