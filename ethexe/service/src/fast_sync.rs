// Copyright (C) Gear Technologies Inc.
// SPDX-License-Identifier: GPL-3.0-or-later WITH Classpath-exception-2.0

//! Fast synchronization for MB-driven ethexe nodes.

use crate::Service;
use alloy::eips::BlockId;
use anyhow::{Context, Result, ensure};
use ethexe_common::{
    Address, BlockData, CodeAndIdUnchecked, Digest, ProgramStates, StateHashWithQueueSize,
    db::{
        BlockMetaStorageRO, CodesStorageRO, CodesStorageRW, ConfigStorageRO, GlobalsStorageRW,
        MbStorageRW, OnChainStorageRW, PreparedBlockData,
    },
    events::{
        BlockEvent, RouterEvent,
        router::{BatchCommittedEvent, EBCommittedEvent, MBCommittedEvent},
    },
    injected,
};
use ethexe_compute::ComputeService;
use ethexe_db::{
    Database,
    iterator::{
        DatabaseIteratorError, DatabaseIteratorStorage, DispatchStashNode, MailboxNode,
        MemoryPagesNode, MemoryPagesRegionNode, MessageQueueNode, ProgramStateNode,
        UserMailboxNode, WaitlistNode,
    },
    visitor::DatabaseVisitor,
};
use ethexe_ethereum::{mirror::MirrorQuery, router::RouterQuery};
use ethexe_malachite::FastSyncReplayTarget;
use ethexe_network::NetworkService;
use ethexe_observer::{ObserverService, utils::BlockLoader};
use ethexe_runtime_common::{
    ScheduleRestorer,
    state::{
        DispatchStash, Mailbox, MemoryPages, MemoryPagesRegion, MessageQueue, ProgramState,
        UserMailbox, Waitlist,
    },
};
use futures::StreamExt;
use gprimitives::{ActorId, CodeId, H256};
use parity_scale_codec::Decode;
use std::{
    cmp::max,
    collections::{BTreeMap, BTreeSet, HashMap},
};

struct EventData {
    latest_committed_batch: Digest,
    replay_target: FastSyncReplayTarget,
}

impl EventData {
    async fn collect(
        block_loader: &impl BlockLoader,
        db: &Database,
        highest_block: H256,
    ) -> Result<Option<Self>> {
        let mut latest_committed_batch = None;
        let mut replay_target = None;

        let mut block = highest_block;
        'blocks: while !db.block_meta(block).prepared {
            let block_data = block_loader.load(block, None).await?;

            let mut pending_eb = None;
            for event in block_data.events.iter().rev() {
                match event {
                    BlockEvent::Router(RouterEvent::BatchCommitted(BatchCommittedEvent {
                        digest,
                    })) => {
                        latest_committed_batch.get_or_insert(*digest);
                    }
                    BlockEvent::Router(RouterEvent::EBCommitted(EBCommittedEvent(eb_hash))) => {
                        pending_eb = Some(*eb_hash);
                    }
                    BlockEvent::Router(RouterEvent::MBCommitted(MBCommittedEvent(mb_hash))) => {
                        if let Some(eb_hash) = pending_eb.take() {
                            replay_target = Some(FastSyncReplayTarget {
                                mb_hash: *mb_hash,
                                eb_hash,
                            });
                            break 'blocks;
                        }
                    }
                    _ => {}
                }
            }

            block = block_data.header.parent_hash;
            if block.is_zero() {
                break;
            }
        }

        let Some(replay_target) = replay_target else {
            return Ok(None);
        };

        Ok(Some(Self {
            // Router.commitBatch emits BatchCommitted before MB/EBCommitted; otherwise we'd seed a bogus previous batch.
            latest_committed_batch: latest_committed_batch
                .context("committed MB replay target without BatchCommitted event")?,
            replay_target,
        }))
    }
}

async fn collect_program_code_ids(
    observer: &mut ObserverService,
    genesis_block: u32,
    latest_committed_block: u32,
) -> Result<BTreeMap<ActorId, CodeId>> {
    Ok(observer
        .router_query()
        .events()
        .program_created()
        .from_block(genesis_block)
        .to_block(latest_committed_block)
        .query()
        .await
        .context("failed to query ProgramCreated events")?
        .into_iter()
        .map(|(event, _log)| (event.actor_id, event.code_id))
        .collect())
}

async fn collect_program_state_hashes(
    observer: &mut ObserverService,
    block_hash: H256,
    program_code_ids: &BTreeMap<ActorId, CodeId>,
) -> Result<BTreeMap<ActorId, H256>> {
    let mut program_states = BTreeMap::new();
    let provider = observer.provider();

    for &actor_id in program_code_ids.keys() {
        let mirror = Address::try_from(actor_id).expect("invalid actor id");
        let mirror = MirrorQuery::new(provider.clone(), mirror);

        let state_hash = mirror.state_hash_at(block_hash).await.with_context(|| {
            format!("failed to get state hash for actor {actor_id} at block {block_hash}")
        })?;

        ensure!(
            !state_hash.is_zero(),
            "state hash is zero for actor {actor_id} at block {block_hash}"
        );

        program_states.insert(actor_id, state_hash);
    }

    Ok(program_states)
}

async fn collect_code_ids(
    observer: &mut ObserverService,
    genesis_block: u32,
    latest_committed_block: u32,
) -> Result<BTreeSet<CodeId>> {
    Ok(observer
        .router_query()
        .events()
        .code_got_validated()
        .valid(true)
        .from_block(genesis_block)
        .to_block(latest_committed_block)
        .query()
        .await
        .context("failed to query CodeGotValidated events")?
        .into_iter()
        .map(|(event, _log)| event.code_id)
        .collect())
}

#[derive(Debug, Clone, Copy, Eq, PartialEq)]
enum RequestMetadata {
    ProgramState,
    MemoryPages,
    MemoryPagesRegion,
    MessageQueue,
    Waitlist,
    Mailbox,
    UserMailbox,
    DispatchStash,
    /// Any data we only insert into the database.
    Data,
}

impl RequestMetadata {
    fn is_data(self) -> bool {
        matches!(self, RequestMetadata::Data)
    }
}

#[derive(Debug)]
struct RequestManager {
    db: Database,

    /// Total completed requests
    total_completed_requests: u64,
    /// Total pending requests
    total_pending_requests: u64,

    /// Pending requests are either:
    /// * Skipped if they are `RequestMetadata::Data` and exist in the database
    /// * Completed if the database has keys
    /// * Converted into one network request
    pending_requests: HashMap<H256, RequestMetadata>,
    /// Completed requests
    responses: Vec<(RequestMetadata, Vec<u8>)>,
}

impl RequestManager {
    fn new(db: Database) -> Self {
        Self {
            db,
            total_completed_requests: 0,
            total_pending_requests: 0,
            pending_requests: HashMap::new(),
            responses: Vec::new(),
        }
    }

    fn add(&mut self, hash: H256, metadata: RequestMetadata) {
        debug_assert_ne!(
            hash,
            H256::zero(),
            "zero hash is cannot be requested from db or network"
        );

        let old_metadata = self.pending_requests.insert(hash, metadata);

        if let Some(old_metadata) = old_metadata {
            debug_assert_eq!(metadata, old_metadata);
        } else {
            self.total_pending_requests += 1;
        }
    }

    async fn request(
        &mut self,
        network: &mut NetworkService,
    ) -> Option<Vec<(RequestMetadata, Vec<u8>)>> {
        let pending_network_requests = self.handle_pending_requests();

        if !pending_network_requests.is_empty() {
            let bitswap = network.bitswap_handle();
            let mut request = bitswap
                .request_many(pending_network_requests.keys().copied())
                .collect();

            let response = loop {
                tokio::select! {
                    _ = network.select_next_some() => {},
                    response = &mut request => break response,
                }
            };
            drop(request);

            self.handle_response(pending_network_requests, response);
        }

        let continue_processing = !(self.pending_requests.is_empty() && self.responses.is_empty());
        if continue_processing {
            let responses: Vec<_> = self.responses.drain(..).collect();
            self.total_completed_requests += responses.len() as u64;
            Some(responses)
        } else {
            None
        }
    }

    fn handle_pending_requests(&mut self) -> HashMap<H256, RequestMetadata> {
        let mut pending_requests = HashMap::new();
        for (hash, metadata) in self.pending_requests.drain() {
            if metadata.is_data() && self.db.cas().contains(hash) {
                self.total_completed_requests += 1;
                continue;
            }

            if let Some(data) = self.db.cas().read(hash) {
                self.responses.push((metadata, data));
                continue;
            }

            pending_requests.insert(hash, metadata);
        }

        pending_requests
    }

    fn handle_response(
        &mut self,
        mut pending_network_requests: HashMap<H256, RequestMetadata>,
        data: Vec<(H256, Vec<u8>)>,
    ) {
        for (hash, data) in data {
            let metadata = pending_network_requests
                .remove(&hash)
                .expect("unknown pending request");

            let db_hash = self.db.cas().write(&data);
            debug_assert_eq!(hash, db_hash);

            self.responses.push((metadata, data));
        }

        debug_assert_eq!(
            pending_network_requests,
            HashMap::new(),
            "network service guarantees it gathers all hashes"
        );
    }

    /// (total completed request, total pending requests)
    fn stats(&self) -> (u64, u64) {
        let completed = self.total_completed_requests;
        let pending = self.total_pending_requests;
        debug_assert!(completed <= pending, "{completed} <= {pending}");
        (completed, pending)
    }
}

impl DatabaseVisitor for RequestManager {
    fn db(&self) -> &dyn DatabaseIteratorStorage {
        &self.db
    }

    fn clone_boxed_db(&self) -> Box<dyn DatabaseIteratorStorage> {
        Box::new(self.db.clone())
    }

    fn on_db_error(&mut self, error: DatabaseIteratorError) {
        let (hash, metadata) = match error {
            DatabaseIteratorError::NoMemoryPages(hash) => {
                (hash.inner(), RequestMetadata::MemoryPages)
            }
            DatabaseIteratorError::NoMemoryPagesRegion(hash) => {
                (hash.inner(), RequestMetadata::MemoryPagesRegion)
            }
            DatabaseIteratorError::NoPageData(hash) => (hash.inner(), RequestMetadata::Data),
            DatabaseIteratorError::NoMessageQueue(hash) => {
                (hash.inner(), RequestMetadata::MessageQueue)
            }
            DatabaseIteratorError::NoWaitlist(hash) => (hash.inner(), RequestMetadata::Waitlist),
            DatabaseIteratorError::NoDispatchStash(hash) => {
                (hash.inner(), RequestMetadata::DispatchStash)
            }
            DatabaseIteratorError::NoMailbox(hash) => (hash.inner(), RequestMetadata::Mailbox),
            DatabaseIteratorError::NoUserMailbox(hash) => {
                (hash.inner(), RequestMetadata::UserMailbox)
            }
            DatabaseIteratorError::NoAllocations(hash) => (hash.inner(), RequestMetadata::Data),
            DatabaseIteratorError::NoProgramState(hash) => (hash, RequestMetadata::ProgramState),
            DatabaseIteratorError::NoPayload(hash) => (hash.inner(), RequestMetadata::Data),
            DatabaseIteratorError::NoOriginalCode(code_id) => {
                (code_id.into(), RequestMetadata::Data)
            }
            DatabaseIteratorError::NoCodeValid(_)
            | DatabaseIteratorError::NoInstrumentedCode(_)
            | DatabaseIteratorError::NoCodeMetadata(_) => return,
            DatabaseIteratorError::NoBlockHeader(_)
            | DatabaseIteratorError::NoBlockEvents(_)
            | DatabaseIteratorError::NoBlockCodesQueue(_)
            | DatabaseIteratorError::NoMb(_)
            | DatabaseIteratorError::NoMbProgramStates(_)
            | DatabaseIteratorError::NoMbSchedule(_)
            | DatabaseIteratorError::NoMbOutcome(_)
            | DatabaseIteratorError::NoProgramCodeId(_) => {
                unreachable!("{error:?}")
            }
        };
        self.add(hash, metadata);
    }
}

impl Drop for RequestManager {
    fn drop(&mut self) {
        #[cfg(debug_assertions)]
        {
            let Self {
                db: _,
                total_completed_requests,
                total_pending_requests,
                pending_requests,
                responses,
            } = self;
            assert_eq!(total_completed_requests, total_pending_requests);
            assert_eq!(*pending_requests, HashMap::new());
            assert_eq!(*responses, Vec::new());
        }
    }
}

/// Synchronize program states and related data from the network.
///
/// This asynchronous function fetches data from the network based on program
/// state hashes and associated metadata using a request-manager mechanism. It also enriches
/// the program states with cached queue sizes.
async fn sync_from_network(
    network: &mut NetworkService,
    db: &Database,
    code_ids: &BTreeSet<CodeId>,
    program_states: BTreeMap<ActorId, H256>,
) -> ProgramStates {
    let mut restored_cached_queue_sizes = BTreeMap::new();

    let mut manager = RequestManager::new(db.clone());

    for &state in program_states.values() {
        manager.add(state, RequestMetadata::ProgramState);
    }

    for &code_id in code_ids {
        manager.add(code_id.into(), RequestMetadata::Data);
    }

    loop {
        let (completed, pending) = manager.stats();
        log::info!("[{completed:>05} / {pending:>05}] Getting network data");

        let Some(responses) = manager.request(network).await else {
            break;
        };

        for (metadata, data) in responses {
            match metadata {
                RequestMetadata::ProgramState => {
                    let state: ProgramState =
                        Decode::decode(&mut &data[..]).expect("bitswap must validate data");
                    let program_state_hash = ethexe_db::hash(&data);
                    restored_cached_queue_sizes.insert(
                        program_state_hash,
                        (
                            state.canonical_queue.cached_queue_size,
                            state.injected_queue.cached_queue_size,
                        ),
                    );
                    ethexe_db::visitor::walk(
                        &mut manager,
                        ProgramStateNode {
                            program_state: state,
                        },
                    );
                }
                RequestMetadata::MemoryPages => {
                    let memory_pages: MemoryPages =
                        Decode::decode(&mut &data[..]).expect("bitswap must validate data");
                    ethexe_db::visitor::walk(&mut manager, MemoryPagesNode { memory_pages });
                }
                RequestMetadata::MemoryPagesRegion => {
                    let memory_pages_region: MemoryPagesRegion =
                        Decode::decode(&mut &data[..]).expect("bitswap must validate data");
                    ethexe_db::visitor::walk(
                        &mut manager,
                        MemoryPagesRegionNode {
                            memory_pages_region,
                        },
                    );
                }
                RequestMetadata::MessageQueue => {
                    let message_queue: MessageQueue =
                        Decode::decode(&mut &data[..]).expect("bitswap must validate data");
                    ethexe_db::visitor::walk(&mut manager, MessageQueueNode { message_queue });
                }
                RequestMetadata::Waitlist => {
                    let waitlist: Waitlist =
                        Decode::decode(&mut &data[..]).expect("bitswap must validate data");
                    ethexe_db::visitor::walk(&mut manager, WaitlistNode { waitlist });
                }
                RequestMetadata::Mailbox => {
                    let mailbox: Mailbox =
                        Decode::decode(&mut &data[..]).expect("bitswap must validate data");
                    ethexe_db::visitor::walk(&mut manager, MailboxNode { mailbox });
                }
                RequestMetadata::UserMailbox => {
                    let user_mailbox: UserMailbox =
                        Decode::decode(&mut &data[..]).expect("bitswap must validate data");
                    ethexe_db::visitor::walk(&mut manager, UserMailboxNode { user_mailbox });
                }
                RequestMetadata::DispatchStash => {
                    let dispatch_stash: DispatchStash =
                        Decode::decode(&mut &data[..]).expect("bitswap must validate data");
                    ethexe_db::visitor::walk(&mut manager, DispatchStashNode { dispatch_stash });
                }
                RequestMetadata::Data => continue,
            }
        }
    }

    log::info!("Network data getting is done");

    // Enrich program states with cached queue size
    program_states
        .into_iter()
        .map(|(program_id, hash)| {
            let (canonical_queue_size, injected_queue_size) = *restored_cached_queue_sizes
                .get(&hash)
                .expect("program state cached queue size must be restored");
            (
                program_id,
                StateHashWithQueueSize {
                    hash,
                    canonical_queue_size,
                    injected_queue_size,
                },
            )
        })
        .collect()
}

/// Instruments a set of codes by delegating their processing to the `ComputeService`.
async fn instrument_codes(
    compute: &mut ComputeService,
    db: &Database,
    mut code_ids: BTreeSet<CodeId>,
) -> Result<()> {
    // `code_valid` is compute's processed marker; valid codes must have instrumented bytes.
    code_ids.retain(|&code_id| db.code_valid(code_id).is_none());

    if code_ids.is_empty() {
        log::info!("No codes to instrument. Skipping...");
        return Ok(());
    }

    log::info!("Instrument {} codes", code_ids.len());

    for &code_id in &code_ids {
        let original_code = db
            .original_code(code_id)
            .expect("`sync_from_network` must fulfill database");
        compute.process_code(CodeAndIdUnchecked {
            code_id,
            code: original_code,
        });
    }

    while let Some(event) = compute.next().await {
        let id = event?.unwrap_code_processed();
        code_ids.remove(&id);
        if code_ids.is_empty() {
            break;
        }
    }

    log::info!("Codes instrumentation done");
    Ok(())
}

async fn set_tx_pool_data_requirement(
    db: &Database,
    block_loader: &impl BlockLoader,
    latest_committed_block_height: u32,
) -> Result<()> {
    let to = latest_committed_block_height as u64;
    let from = to.saturating_sub(injected::VALIDITY_WINDOW as u64);

    // TODO: #4926 unsafe solution - we need it for taking events from predecessor blocks in ethexe-compute
    let blocks = block_loader.load_many(from..=to).await?;
    for BlockData {
        hash,
        header,
        events,
    } in blocks.into_values()
    {
        db.set_block_header(hash, header);
        db.set_block_events(hash, &events);
    }

    Ok(())
}

async fn latest_era_with_committed_validators(
    db: &Database,
    router: &RouterQuery,
    block_hash: H256,
) -> Result<u64> {
    let storage_view = router.storage_view_at(block_hash).await?;

    db.config()
        .timelines
        .era_from_ts(max(
            storage_view
                .validationSettings
                .validators0
                .useFromTimestamp
                .to::<u64>(),
            storage_view
                .validationSettings
                .validators1
                .useFromTimestamp
                .to::<u64>(),
        ))
        .context("failed to calculate era from validators timestamp")
}

pub(crate) async fn sync(service: &mut Service) -> Result<()> {
    let Service {
        observer,
        compute,
        network,
        malachite,
        db,
        #[cfg(test)]
        sender,
        ..
    } = service;
    let Some(network) = network else {
        log::warn!("Network service is disabled. Skipping fast synchronization...");
        return Ok(());
    };

    log::info!("Fast synchronization is in progress...");

    let finalized_block = observer
        .block_loader()
        // we get finalized block to avoid block reorganization
        // because we restore the database only for the latest block of a chain,
        // and thus the reorganization can lead us to an empty block
        .load_simple(BlockId::finalized())
        .await
        .context("failed to get latest finalized block")?
        .hash;

    let genesis_block_hash = db.config().genesis_block_hash;
    let genesis_block = observer
        .block_loader()
        .load_simple(genesis_block_hash)
        .await
        .context("failed to get genesis block")?;

    let block_loader = observer.block_loader();
    let Some(event_data) = EventData::collect(&block_loader, db, finalized_block).await? else {
        log::warn!("No finalized committed MB found. Skipping fast synchronization...");
        return Ok(());
    };

    let EventData {
        latest_committed_batch,
        replay_target,
    } = event_data;

    let FastSyncReplayTarget {
        mb_hash: latest_committed_mb,
        eb_hash: latest_committed_eb,
    } = replay_target;
    let block_data = block_loader.load(latest_committed_eb, None).await?;

    let code_ids = collect_code_ids(
        observer,
        genesis_block.header.height,
        block_data.header.height,
    )
    .await?;
    let program_code_ids = collect_program_code_ids(
        observer,
        genesis_block.header.height,
        block_data.header.height,
    )
    .await?;
    let program_state_hashes =
        collect_program_state_hashes(observer, finalized_block, &program_code_ids).await?;

    let program_states = sync_from_network(network, db, &code_ids, program_state_hashes).await;

    instrument_codes(compute, db, code_ids).await?;

    let schedule = ScheduleRestorer::from_storage(db, &program_states)?.restore();

    set_tx_pool_data_requirement(db, &block_loader, block_data.header.height).await?;

    for (program_id, code_id) in program_code_ids {
        db.set_program_code_id(program_id, code_id);
    }

    let latest_era_with_committed_validators =
        latest_era_with_committed_validators(db, &observer.router_query(), latest_committed_eb)
            .await?;

    ethexe_common::setup_block_in_db(
        db,
        latest_committed_eb,
        PreparedBlockData {
            header: block_data.header,
            events: block_data.events.clone(),
            latest_era_with_committed_validators,
            // NOTE: there is no invariant that fast sync should recover codes queue
            codes_queue: Default::default(),
            // TODO #4812: using `latest_committed_batch` here is not correct,
            // because `latest_committed_batch` is latest for finalized block, not for `block_hash`.
            last_committed_batch: latest_committed_batch,
            last_committed_mb: latest_committed_mb,
            last_committed_eb: latest_committed_eb,
        },
    );

    db.mutate_mb_meta(latest_committed_mb, |meta| {
        meta.computed = true;
        meta.last_advanced_eb = latest_committed_eb;
    });

    // NOTE: Malachite should restore `CompactMb` by itself
    //db.set_mb_compact_block(latest_committed_mb, compact);

    db.set_mb_program_states(latest_committed_mb, program_states.clone());
    db.set_mb_schedule(latest_committed_mb, schedule);
    // The committed fast-sync anchor is never replayed for batch creation, so
    // keep queue-derived outcome invariants out of restoration and persist an
    // empty row only to satisfy database walks over committed MB metadata.
    db.set_mb_outcome(latest_committed_mb, Default::default());

    db.globals_mutate(|globals| {
        globals.start_block_hash = latest_committed_eb;
        globals.latest_synced_eb = block_data.to_simple();
        globals.latest_prepared_eb_hash = latest_committed_eb;
        globals.latest_finalized_mb_hash = latest_committed_mb;
        globals.latest_computed_mb_hash = latest_committed_mb;
    });

    if let Some(malachite) = malachite.as_mut() {
        // `Service::run` performs fast sync before `run_inner().start_app_task()`,
        // so live Malachite callbacks cannot race this startup replay gate.
        let _ = malachite.enable_fast_sync_replay_filter(replay_target)?;
        malachite.receive_new_chain_head(block_data.to_simple());
    }

    log::info!(
        "Fast synchronization done: synced to {latest_committed_eb:?}, height {height:?}, MB {latest_committed_mb}",
        height = block_data.header.height
    );

    #[cfg(test)]
    sender
        .send(crate::tests::utils::TestingEvent::FastSyncDone(
            replay_target,
        ))
        .await;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use ethexe_common::{BlockHeader, Digest, SimpleBlockData};
    use ethexe_ethereum::IntoBlockId;
    use ethexe_processor::Processor;
    use std::{
        collections::{BTreeMap, BTreeSet, HashMap},
        ops::RangeInclusive,
    };

    #[derive(Default)]
    struct TestBlockLoader {
        blocks: BTreeMap<H256, BlockData>,
    }

    impl BlockLoader for TestBlockLoader {
        async fn load_simple(&self, _block: impl IntoBlockId) -> Result<SimpleBlockData> {
            anyhow::bail!("load_simple is not used by EventData::collect tests")
        }

        async fn load(&self, hash: H256, _header: Option<BlockHeader>) -> Result<BlockData> {
            self.blocks
                .get(&hash)
                .cloned()
                .with_context(|| format!("missing test block {hash}"))
        }

        async fn load_many(&self, _range: RangeInclusive<u64>) -> Result<HashMap<H256, BlockData>> {
            anyhow::bail!("load_many is not used by EventData::collect tests")
        }
    }

    #[tokio::test]
    async fn instrument_codes_skips_already_processed_codes() {
        let db = Database::memory();
        let code_id = CodeId::from([1; 32]);
        db.set_code_valid(code_id, true);
        let processor = Processor::new(db.clone()).unwrap();
        let mut compute = ComputeService::new(db.clone(), processor);

        instrument_codes(&mut compute, &db, BTreeSet::from([code_id]))
            .await
            .unwrap();
    }

    fn test_block(hash: H256, parent_hash: H256, events: Vec<BlockEvent>) -> BlockData {
        BlockData {
            hash,
            header: BlockHeader {
                height: hash.to_low_u64_be() as u32,
                timestamp: 0,
                parent_hash,
            },
            events,
        }
    }

    #[tokio::test]
    async fn event_data_collects_committed_chain_as_single_pair() {
        let db = Database::memory();
        let older_block = H256::from_low_u64_be(1);
        let newer_block = H256::from_low_u64_be(2);
        let older_mb = H256::repeat_byte(0x11);
        let older_eb = H256::repeat_byte(0x22);
        let newer_mb = H256::repeat_byte(0x33);
        let newer_eb = H256::repeat_byte(0x44);
        let newer_batch = Digest::random();

        let mut loader = TestBlockLoader::default();
        loader.blocks.insert(
            newer_block,
            test_block(
                newer_block,
                older_block,
                vec![
                    BlockEvent::Router(RouterEvent::MBCommitted(MBCommittedEvent(newer_mb))),
                    BlockEvent::Router(RouterEvent::EBCommitted(EBCommittedEvent(newer_eb))),
                    BlockEvent::Router(RouterEvent::BatchCommitted(BatchCommittedEvent {
                        digest: newer_batch,
                    })),
                ],
            ),
        );
        loader.blocks.insert(
            older_block,
            test_block(
                older_block,
                H256::zero(),
                vec![
                    BlockEvent::Router(RouterEvent::MBCommitted(MBCommittedEvent(older_mb))),
                    BlockEvent::Router(RouterEvent::EBCommitted(EBCommittedEvent(older_eb))),
                ],
            ),
        );

        let event_data = EventData::collect(&loader, &db, newer_block)
            .await
            .unwrap()
            .expect("committed chain found");

        assert_eq!(event_data.replay_target.mb_hash, newer_mb);
        assert_eq!(event_data.replay_target.eb_hash, newer_eb);
        assert_eq!(event_data.latest_committed_batch, newer_batch);
    }

    #[tokio::test]
    async fn event_data_skips_newer_mb_without_eb_anchor() {
        let db = Database::memory();
        let older_block = H256::from_low_u64_be(1);
        let newer_block = H256::from_low_u64_be(2);
        let older_mb = H256::repeat_byte(0x11);
        let older_eb = H256::repeat_byte(0x22);
        let newer_mb = H256::repeat_byte(0x33);
        let older_batch = Digest::random();

        let mut loader = TestBlockLoader::default();
        loader.blocks.insert(
            newer_block,
            test_block(
                newer_block,
                older_block,
                vec![BlockEvent::Router(RouterEvent::MBCommitted(
                    MBCommittedEvent(newer_mb),
                ))],
            ),
        );
        loader.blocks.insert(
            older_block,
            test_block(
                older_block,
                H256::zero(),
                vec![
                    BlockEvent::Router(RouterEvent::MBCommitted(MBCommittedEvent(older_mb))),
                    BlockEvent::Router(RouterEvent::EBCommitted(EBCommittedEvent(older_eb))),
                    BlockEvent::Router(RouterEvent::BatchCommitted(BatchCommittedEvent {
                        digest: older_batch,
                    })),
                ],
            ),
        );

        let event_data = EventData::collect(&loader, &db, newer_block)
            .await
            .unwrap()
            .expect("committed chain found");

        assert_eq!(event_data.replay_target.mb_hash, older_mb);
        assert_eq!(event_data.replay_target.eb_hash, older_eb);
        assert_eq!(event_data.latest_committed_batch, older_batch);
    }

    #[tokio::test]
    async fn event_data_rejects_committed_chain_without_batch_digest() {
        let db = Database::memory();
        let block = H256::from_low_u64_be(1);
        let mb = H256::repeat_byte(0x11);
        let eb = H256::repeat_byte(0x22);

        let mut loader = TestBlockLoader::default();
        loader.blocks.insert(
            block,
            test_block(
                block,
                H256::zero(),
                vec![
                    BlockEvent::Router(RouterEvent::MBCommitted(MBCommittedEvent(mb))),
                    BlockEvent::Router(RouterEvent::EBCommitted(EBCommittedEvent(eb))),
                ],
            ),
        );

        let Err(err) = EventData::collect(&loader, &db, block).await else {
            panic!("expected missing batch digest error");
        };

        assert!(
            err.to_string()
                .contains("committed MB replay target without BatchCommitted event")
        );
    }

    #[tokio::test]
    async fn event_data_ignores_committed_mb_without_eb_anchor() {
        let db = Database::memory();
        let block = H256::from_low_u64_be(1);
        let mb = H256::repeat_byte(0x11);

        let mut loader = TestBlockLoader::default();
        loader.blocks.insert(
            block,
            test_block(
                block,
                H256::zero(),
                vec![BlockEvent::Router(RouterEvent::MBCommitted(
                    MBCommittedEvent(mb),
                ))],
            ),
        );

        assert!(
            EventData::collect(&loader, &db, block)
                .await
                .unwrap()
                .is_none()
        );
    }
}
