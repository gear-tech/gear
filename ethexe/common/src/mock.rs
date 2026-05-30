// Copyright (C) Gear Technologies Inc.
// SPDX-License-Identifier: GPL-3.0-or-later WITH Classpath-exception-2.0

use crate::{
    Address, BlockData, BlockHeader, CodeBlobInfo, Digest, HashOf, ProgramStates,
    ProtocolTimelines, Rfm, Schedule, ScheduledTask, Sd, SimpleBlockData, StateHashWithQueueSize,
    Sum, ValidatorsVec,
    consensus::BatchCommitmentValidationRequest,
    db::*,
    ecdsa::{PrivateKey, SignedMessage},
    events::BlockEvent,
    gear::{
        BatchCommitment, ChainCommitment, CodeCommitment, Message, MessageType, StateTransition,
    },
    injected::{AddressedInjectedTransaction, InjectedTransaction, Promise},
    malachite::Transactions,
};
use alloc::{collections::BTreeMap, vec};
use gear_core::{
    code::{CodeMetadata, InstrumentedCode},
    limited::LimitedVec,
    message::{ReplyCode, SuccessReplyReason},
    rpc::ReplyInfo,
    tasks::ScheduledTask as CoreScheduledTask,
};
use gprimitives::{ActorId, CodeId, H256, MessageId, ReservationId};
use itertools::Itertools;
use proptest::{
    arbitrary::Arbitrary,
    collection,
    prelude::{BoxedStrategy, Strategy, any},
    prop_oneof,
    strategy::{Just, ValueTree},
    test_runner::TestRunner,
};
use std::collections::VecDeque;
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

/// Constructs a mock value of `Self` from the given `args`.
///
/// Implemented automatically for every type that satisfies `Arbitrary` via the
/// blanket impl below. Explicit impls are provided for types that need custom
/// behaviour (e.g. [`Promise`]).
pub trait Mock<Args = ()> {
    /// Returns a mock instance of `Self`.
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

/// Parameters controlling how a [`BlockHeader`] mock is generated.
///
/// When `parent_hash` is `None` a random value is chosen; supply a concrete
/// hash to produce a header that chains from a known predecessor.
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

/// Parameters controlling how a [`ChainCommitment`] mock is generated.
///
/// When `head` is `None` a random block hash is used; supply one to anchor the
/// commitment to a known chain head.
#[derive(Debug, Clone, Copy, Default)]
pub struct ChainCommitmentParams {
    head: Option<H256>,
}

impl From<()> for ChainCommitmentParams {
    fn from((): ()) -> Self {
        Self::default()
    }
}

impl From<H256> for ChainCommitmentParams {
    fn from(head: H256) -> Self {
        Self { head: Some(head) }
    }
}

/// Parameters controlling how a [`BlockChain`] mock is generated.
///
/// `len` is the number of non-genesis blocks; `validators` is the validator set
/// written to the DB for each era that the generated blocks span.
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

/// Parameters controlling how an [`AddressedInjectedTransaction`] mock is generated.
///
/// When `signer` is `None` a random secp256k1 private key is drawn; supply one
/// to produce a transaction signed by a known key.
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

fn reservation_id_strategy() -> BoxedStrategy<ReservationId> {
    any::<[u8; 32]>().prop_map(Into::into).boxed()
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

/// Returns a proptest strategy that generates an arbitrary [`ScheduledTask`] variant.
pub fn scheduled_task_strategy() -> BoxedStrategy<ScheduledTask> {
    prop_oneof![
        (
            actor_id_strategy(),
            actor_id_strategy(),
            message_id_strategy()
        )
            .prop_map(|(program_id, user_id, message_id)| {
                CoreScheduledTask::<Rfm, Sd, Sum>::RemoveFromMailbox(
                    (program_id, user_id),
                    message_id,
                )
            }),
        (actor_id_strategy(), message_id_strategy()).prop_map(|(program_id, message_id)| {
            CoreScheduledTask::<Rfm, Sd, Sum>::RemoveFromWaitlist(program_id, message_id)
        }),
        (actor_id_strategy(), message_id_strategy()).prop_map(|(program_id, message_id)| {
            CoreScheduledTask::<Rfm, Sd, Sum>::WakeMessage(program_id, message_id)
        }),
        (actor_id_strategy(), message_id_strategy()).prop_map(|(program_id, message_id)| {
            CoreScheduledTask::<Rfm, Sd, Sum>::SendDispatch((program_id, message_id))
        }),
        (message_id_strategy(), actor_id_strategy()).prop_map(|(message_id, to_mailbox)| {
            CoreScheduledTask::<Rfm, Sd, Sum>::SendUserMessage {
                message_id,
                to_mailbox,
            }
        }),
        (actor_id_strategy(), reservation_id_strategy()).prop_map(
            |(program_id, reservation_id)| {
                CoreScheduledTask::<Rfm, Sd, Sum>::RemoveGasReservation(program_id, reservation_id)
            }
        ),
    ]
    .boxed()
}

/// Returns a proptest strategy that generates an arbitrary [`Schedule`] (a `BTreeMap` of
/// block-number to task sets).
pub fn schedule_strategy() -> BoxedStrategy<Schedule> {
    collection::btree_map(
        any::<u32>(),
        collection::btree_set(scheduled_task_strategy(), 0..=4),
        0..=4,
    )
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
            era: 1000.try_into().unwrap(),
            election: 200,
            slot: 10.try_into().unwrap(),
        })
        .boxed()
    }
}

impl Arbitrary for MessageType {
    type Parameters = ();
    type Strategy = BoxedStrategy<Self>;

    fn arbitrary_with(_args: Self::Parameters) -> Self::Strategy {
        prop_oneof![Just(Self::Canonical), Just(Self::Injected)].boxed()
    }
}

impl Arbitrary for StateHashWithQueueSize {
    type Parameters = ();
    type Strategy = BoxedStrategy<Self>;

    fn arbitrary_with(_args: Self::Parameters) -> Self::Strategy {
        (h256_strategy(), any::<u8>(), any::<u8>())
            .prop_map(|(hash, canonical_queue_size, injected_queue_size)| Self {
                hash,
                canonical_queue_size,
                injected_queue_size,
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
        let head = match args.head {
            Some(head) => Just(head).boxed(),
            None => h256_strategy(),
        };

        (
            StateTransition::arbitrary_with(()),
            StateTransition::arbitrary_with(()),
            head,
        )
            .prop_map(|(first, second, head)| Self {
                transitions: vec![first, second],
                head,
                last_advanced_eth_block: H256::zero(),
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
            h256_strategy(),
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

impl Mock<()> for Promise {
    fn mock(_args: ()) -> Self {
        Promise::mock(HashOf::random())
    }
}

impl Mock<HashOf<InjectedTransaction>> for Promise {
    fn mock(tx_hash: HashOf<InjectedTransaction>) -> Self {
        Promise {
            tx_hash,
            reply: ReplyInfo {
                payload: H256::random().0.to_vec(),
                value: 42,
                code: ReplyCode::Success(SuccessReplyReason::Manual),
            },
        }
    }
}

/// On-chain data available after a block has been synced from Ethereum.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SyncedBlockData {
    /// The block header (height, timestamp, parent hash).
    pub header: BlockHeader,
    /// Decoded on-chain events emitted in this block.
    pub events: Vec<BlockEvent>,
}

/// Data recorded after the block preparation phase has completed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PreparedBlockData {
    /// Queue of code IDs awaiting on-chain validation at this block.
    pub codes_queue: VecDeque<CodeId>,
    /// Digest of the last batch commitment committed before this block.
    pub last_committed_batch: Digest,
    /// Hash of the last committed micro-block at this block height.
    pub last_committed_mb: H256,
}

/// Complete mock representation of a single Ethereum block, combining both
/// synced and preparation-phase data used by tests that drive the full pipeline.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BlockFullData {
    /// Block hash.
    pub hash: H256,
    /// Data available after the sync phase; `None` if not yet synced.
    pub synced: Option<SyncedBlockData>,
    /// Data available after the preparation phase; `None` if not yet prepared.
    pub prepared: Option<PreparedBlockData>,
}

impl BlockFullData {
    /// Returns the synced data. Panics if the block has not been synced.
    #[track_caller]
    pub fn as_synced(&self) -> &SyncedBlockData {
        self.synced.as_ref().expect("block not synced")
    }

    /// Returns a mutable reference to the synced data. Panics if the block has not been synced.
    #[track_caller]
    pub fn as_synced_mut(&mut self) -> &mut SyncedBlockData {
        self.synced.as_mut().expect("block not synced")
    }

    /// Returns the preparation-phase data. Panics if the block has not been prepared.
    #[track_caller]
    pub fn as_prepared(&self) -> &PreparedBlockData {
        self.prepared.as_ref().expect("block is not prepared")
    }

    /// Returns a mutable reference to the preparation-phase data. Panics if the block has not been prepared.
    #[track_caller]
    pub fn as_prepared_mut(&mut self) -> &mut PreparedBlockData {
        self.prepared.as_mut().expect("block is not prepared")
    }

    /// Constructs a [`SimpleBlockData`] from this block's hash and synced header.
    ///
    /// Panics if the block has not been synced.
    #[track_caller]
    pub fn to_simple(&self) -> SimpleBlockData {
        SimpleBlockData {
            hash: self.hash,
            header: self.as_synced().header,
        }
    }
}

/// Instrumented code together with its metadata, as stored after successful validation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InstrumentedCodeData {
    /// Gas-metered and stack-checked WASM code ready for execution.
    pub instrumented: InstrumentedCode,
    /// Metadata associated with the code (e.g. limits, version).
    pub meta: CodeMetadata,
}

/// All data associated with a code blob in the mock database.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CodeData {
    /// Raw WASM bytes as received from the beacon chain blob.
    pub original_bytes: Vec<u8>,
    /// Ethereum transaction metadata for the blob submission: timestamp of the containing block and the transaction hash.
    pub blob_info: CodeBlobInfo,
    /// Instrumented code and metadata; `None` if the code has not been validated yet.
    pub instrumented: Option<InstrumentedCodeData>,
}

impl CodeData {
    /// Returns the instrumented code data. Panics if the code has not been instrumented.
    pub fn as_instrumented(&self) -> &InstrumentedCodeData {
        self.instrumented.as_ref().expect("code not instrumented")
    }

    /// Returns a mutable reference to the instrumented code data. Panics if the code has not been instrumented.
    pub fn as_instrumented_mut(&mut self) -> &mut InstrumentedCodeData {
        self.instrumented.as_mut().expect("code not instrumented")
    }
}

/// Computed-side payload for an [`MbFullData`]. `None` on
/// [`MbFullData::computed`] means `mb_meta.computed = false` and
/// `mb_program_states` is left unwritten.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct MockComputedMbData {
    /// Program state hashes and queue sizes after this micro-block was computed.
    pub program_states: ProgramStates,
}

/// One MB entry in the [`BlockChain`] mock. Paralleled with
/// [`BlockChain::blocks`]: `mbs[i]` corresponds to `blocks[i]`. The
/// first entry (`mbs[0]`) is a sentinel with `hash = H256::zero()`
/// (mirrors the `blocks[0]` genesis-parent placeholder) and is not
/// written to the DB.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MbFullData {
    pub hash: H256,
    /// Parent MB hash. `H256::zero()` for the very first real MB.
    pub parent: H256,
    /// MB height. Set to the block index `i` so it monotonically
    /// matches [`BlockChain::blocks`].
    pub height: u64,
    /// Computed-side data. `Some(default)` by default; setting to
    /// `None` skips writing `mb_program_states` and `mb_meta.computed`.
    pub computed: Option<MockComputedMbData>,
    /// SCALE-encoded transactions blob to write under this MB.
    /// Defaults to an empty list. Tests that need specific txs in the
    /// dedup-window walk (e.g. tx_validity::Duplicate) can set this.
    pub transactions: Transactions,
}

impl MbFullData {
    /// Returns the computed MB data. Panics if this MB has not been marked as computed.
    #[track_caller]
    pub fn as_computed(&self) -> &MockComputedMbData {
        self.computed
            .as_ref()
            .expect("MB not marked computed in this mock chain")
    }

    /// Returns a mutable reference to the computed MB data. Panics if this MB has not been marked as computed.
    #[track_caller]
    pub fn as_computed_mut(&mut self) -> &mut MockComputedMbData {
        self.computed
            .as_mut()
            .expect("MB not marked computed in this mock chain")
    }
}

/// In-memory mock of an entire Ethereum block chain with associated MB chain and code store.
///
/// Used in proptest and integration tests to populate a database via [`BlockChain::setup`]
/// and then exercise the ethexe pipeline against a realistic, self-consistent state.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BlockChain {
    /// Ordered sequence of Ethereum blocks; `blocks[0]` is a genesis-parent sentinel.
    pub blocks: VecDeque<BlockFullData>,
    /// One MB per `blocks[i]`. `mbs[0]` is a sentinel — see [`MbFullData`].
    pub mbs: VecDeque<MbFullData>,
    /// Code blobs keyed by code ID.
    pub codes: BTreeMap<CodeId, CodeData>,
    /// Validator set written to the DB for every era spanned by the chain.
    pub validators: ValidatorsVec,
    /// DB configuration written by [`BlockChain::setup`].
    pub config: DBConfig,
    /// DB globals written by [`BlockChain::setup`].
    pub globals: DBGlobals,
}

impl BlockChain {
    /// `mbs[idx]` accessor. Panics on out-of-range — mirrors the
    /// existing direct field access for `blocks[idx]`.
    #[track_caller]
    pub fn mb_at(&self, idx: usize) -> &MbFullData {
        &self.mbs[idx]
    }

    /// `mbs[idx]` mutable accessor. Panics on out-of-range.
    #[track_caller]
    pub fn mb_at_mut(&mut self, idx: usize) -> &mut MbFullData {
        &mut self.mbs[idx]
    }

    /// Convenience for the common `mbs[idx].hash` pattern.
    #[track_caller]
    pub fn mb_hash_at(&self, idx: usize) -> H256 {
        self.mbs[idx].hash
    }
}

impl BlockChain {
    /// Writes all blocks, MB rows, codes, validators, config, and globals into `db`
    /// and returns `self` unchanged so callers can chain further setup steps.
    #[track_caller]
    pub fn setup<DB>(self, db: &DB) -> Self
    where
        DB: BlockMetaStorageRW
            + OnChainStorageRW
            + CodesStorageRW
            + MbStorageRW
            + SetConfig
            + SetGlobals,
    {
        let BlockChain {
            blocks,
            mbs,
            codes,
            validators,
            config,
            globals,
        } = self.clone();

        db.set_config(config.clone());
        db.set_globals(globals);

        // Write MB rows in chronological order. Skip the index-0
        // sentinel (zero hash). Empty-transactions MBs share one CAS
        // entry naturally — `set_transactions` is content-addressed.
        for mb in &mbs {
            if mb.hash == H256::zero() {
                continue;
            }
            let transactions_hash = db.set_transactions(mb.transactions.clone());
            db.set_mb_compact_block(
                mb.hash,
                CompactMb {
                    parent: mb.parent,
                    height: mb.height,
                    transactions_hash,
                },
            );
            if let Some(computed) = &mb.computed {
                db.set_mb_program_states(mb.hash, computed.program_states.clone());
                db.mutate_mb_meta(mb.hash, |meta| {
                    meta.computed = true;
                });
            }
        }

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

                let block_era = config.timelines.era_from_ts(header.timestamp).unwrap();
                db.set_validators(block_era, validators.clone());
                db.mutate_block_meta(hash, |meta| {
                    meta.latest_era_validators_committed = Some(block_era)
                });
            }

            if let Some(PreparedBlockData {
                codes_queue,
                last_committed_batch,
                last_committed_mb,
            }) = prepared
            {
                db.mutate_block_meta(hash, |meta| {
                    *meta = BlockMeta {
                        prepared: true,
                        codes_queue: Some(codes_queue),
                        last_committed_batch: Some(last_committed_batch),
                        last_committed_mb: Some(last_committed_mb),
                        ..*meta
                    };
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
        let blocks: VecDeque<_> = (0..len + 2)
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
                            last_committed_batch: Digest::zero(),
                            last_committed_mb: H256::zero(),
                        }),
                    }
                },
            )
            .collect();

        let config = DBConfig {
            version: 0,
            chain_id: 0,
            router_address,
            timelines: ProtocolTimelines {
                genesis_ts: genesis_ts as u64,
                era: (slot * 100).try_into().unwrap(),
                election: slot * 20,
                slot: slot.try_into().unwrap(),
            },
            genesis_block_hash: blocks[0].hash,
            max_validators: 10,
        };

        // Build a parallel MB chain. `mbs[0]` is a sentinel matching
        // the `blocks[0]` genesis-parent placeholder; subsequent MBs
        // link parent-to-parent in chronological order.
        let mut mbs: VecDeque<MbFullData> = VecDeque::with_capacity(blocks.len());
        let mut prev_mb_hash = H256::zero();
        for i in 0..blocks.len() {
            if i == 0 {
                mbs.push_back(MbFullData {
                    hash: H256::zero(),
                    parent: H256::zero(),
                    height: 0,
                    computed: None,
                    transactions: Transactions::new(vec![]),
                });
                continue;
            }
            // Synthetic but stable, non-zero hash distinct from block hashes.
            let mut hb = [0u8; 32];
            hb[0] = 0xCD;
            hb[1..9].copy_from_slice(&(i as u64).to_be_bytes());
            let hash = H256::from(hb);
            mbs.push_back(MbFullData {
                hash,
                parent: prev_mb_hash,
                height: i as u64,
                computed: Some(MockComputedMbData::default()),
                transactions: Transactions::new(vec![]),
            });
            prev_mb_hash = hash;
        }

        // NOTE: `latest_{finalized,computed}_mb_hash` default to zero
        // so existing tests that only set up blocks (and not an MB
        // chain) keep their old "no MBs committed yet" semantics.
        // Tests that exercise the MB pipeline must explicitly
        // `tap_mut(|c| c.globals.latest_computed_mb_hash = c.mb_hash_at(N))`.
        let globals = DBGlobals {
            start_block_hash: blocks[0].hash,
            latest_synced_eb: blocks.back().unwrap().to_simple(),
            latest_prepared_eb_hash: blocks.back().unwrap().hash,
            latest_finalized_mb_hash: H256::zero(),
            latest_computed_mb_hash: H256::zero(),
        };

        Self {
            blocks,
            mbs,
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

impl SimpleBlockData {
    /// Writes this block's header and an empty event list into `db`, marks it synced, and returns `self`.
    pub fn setup<DB>(self, db: &DB) -> Self
    where
        DB: OnChainStorageRW,
    {
        db.set_block_header(self.hash, self.header);
        db.set_block_events(self.hash, &[]);
        db.set_block_synced(self.hash);
        self
    }

    /// Returns a new [`SimpleBlockData`] that immediately follows this block.
    ///
    /// The hash is incremented by one and the timestamp advances by 10 units.
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
    /// Writes this block's header and events into `db`, marks it synced, and returns `self`.
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
        (ProtocolTimelines::arbitrary_with(()), h256_strategy())
            .prop_map(|(timelines, genesis_block_hash)| Self {
                version: 0,
                chain_id: 0,
                router_address: Address::default(),
                timelines,
                genesis_block_hash,
                max_validators: 0,
            })
            .boxed()
    }
}

impl Arbitrary for DBGlobals {
    type Parameters = ();
    type Strategy = BoxedStrategy<Self>;

    fn arbitrary_with(_args: Self::Parameters) -> Self::Strategy {
        (
            h256_strategy(),
            SimpleBlockData::arbitrary_with(().into()),
            h256_strategy(),
            h256_strategy(),
            h256_strategy(),
        )
            .prop_map(
                |(
                    start_block_hash,
                    latest_synced_eb,
                    latest_prepared_eb_hash,
                    latest_finalized_mb_hash,
                    latest_computed_mb_hash,
                )| Self {
                    start_block_hash,
                    latest_synced_eb,
                    latest_prepared_eb_hash,
                    latest_finalized_mb_hash,
                    latest_computed_mb_hash,
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
