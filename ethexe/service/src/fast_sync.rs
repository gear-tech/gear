// Copyright (C) Gear Technologies Inc.
// SPDX-License-Identifier: GPL-3.0-or-later WITH Classpath-exception-2.0

//! Fast synchronization for MB-driven ethexe nodes.

use crate::Service;
use alloy::eips::BlockId;
use anyhow::{Context, Result, bail, ensure};
use ethexe_common::{
    BlockData, CodeAndIdUnchecked, Digest, ProgramStates, SimpleBlockData,
    db::{
        BlockMetaStorageRO, CodesStorageRO, CodesStorageRW, CompactMb, ConfigStorageRO,
        GlobalsStorageRW, MbMeta, MbStorageRO, MbStorageRW, OnChainStorageRW, PreparedBlockData,
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
        MbOutcomeNode, MbProgramStatesNode, MbScheduleNode, MemoryPagesNode, MemoryPagesRegionNode,
        MessageQueueNode, ProgramStateNode, UserMailboxNode, WaitlistNode,
    },
    visitor::DatabaseVisitor,
};
use ethexe_ethereum::router::RouterQuery;
use ethexe_network::NetworkService;
use ethexe_observer::{ObserverService, utils::BlockLoader};
use ethexe_runtime_common::state::{
    DispatchStash, Mailbox, MemoryPages, MemoryPagesRegion, MessageQueue, ProgramState,
    UserMailbox, Waitlist,
};
use futures::StreamExt;
use gprimitives::{ActorId, CodeId, H256};
use parity_scale_codec::Decode;
use std::{
    cmp::max,
    collections::{BTreeMap, BTreeSet, HashMap},
};
use tokio::time::{Duration, timeout};

struct EventData {
    latest_committed_batch: Digest,
    committed_mbs: Vec<H256>,
    latest_committed_eb: Option<H256>,
}

impl EventData {
    async fn collect(
        block_loader: &impl BlockLoader,
        db: &Database,
        highest_block: H256,
    ) -> Result<Option<Self>> {
        let mut latest_committed_batch = None;
        let mut committed_mbs = Vec::new();
        let mut latest_committed_eb = None;

        let mut block = highest_block;
        while !db.block_meta(block).prepared {
            let block_data = block_loader.load(block, None).await?;

            for event in block_data.events.iter().rev() {
                match event {
                    BlockEvent::Router(RouterEvent::BatchCommitted(BatchCommittedEvent {
                        digest,
                    })) if latest_committed_batch.is_none() => {
                        latest_committed_batch = Some(*digest);
                    }
                    BlockEvent::Router(RouterEvent::MBCommitted(MBCommittedEvent(mb_hash))) => {
                        committed_mbs.push(*mb_hash);
                    }
                    BlockEvent::Router(RouterEvent::EBCommitted(EBCommittedEvent(eb_hash)))
                        if latest_committed_eb.is_none() =>
                    {
                        latest_committed_eb = Some(*eb_hash);
                    }
                    _ => {}
                }
            }

            block = block_data.header.parent_hash;
            if block.is_zero() {
                break;
            }
        }

        if committed_mbs.is_empty() {
            return Ok(None);
        }

        Ok(Some(Self {
            latest_committed_batch: latest_committed_batch.unwrap_or_default(),
            committed_mbs,
            latest_committed_eb,
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
    Data,
}

impl RequestMetadata {
    fn is_data(self) -> bool {
        matches!(self, Self::Data)
    }
}

#[derive(Debug)]
struct RequestManager {
    db: Database,
    total_completed_requests: u64,
    total_pending_requests: u64,
    pending_requests: HashMap<H256, RequestMetadata>,
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
        debug_assert_ne!(hash, H256::zero(), "zero hash cannot be requested");

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
    ) -> Result<Option<Vec<(RequestMetadata, Vec<u8>)>>> {
        let pending_network_requests = self.handle_pending_requests();

        if !pending_network_requests.is_empty() {
            let mut response = Vec::with_capacity(pending_network_requests.len());
            for &hash in pending_network_requests.keys() {
                response.push((hash, network.bitswap_fetch_hash(hash).await));
            }

            self.handle_response(pending_network_requests, response)?;
        }

        let continue_processing = !(self.pending_requests.is_empty() && self.responses.is_empty());
        if continue_processing {
            let responses = self.responses.drain(..).collect::<Vec<_>>();
            self.total_completed_requests += responses.len() as u64;
            Ok(Some(responses))
        } else {
            Ok(None)
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
    ) -> Result<()> {
        for (hash, data) in data {
            let metadata = pending_network_requests
                .remove(&hash)
                .expect("unknown pending request");

            let db_hash = self.db.cas().write(&data);
            ensure!(
                hash == db_hash,
                "bitswap returned data with unexpected hash"
            );

            self.responses.push((metadata, data));
        }

        debug_assert!(
            pending_network_requests.is_empty(),
            "network service must gather all requested hashes"
        );
        Ok(())
    }

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
            assert_eq!(self.total_completed_requests, self.total_pending_requests);
            assert!(self.pending_requests.is_empty());
            assert!(self.responses.is_empty());
        }
    }
}

async fn sync_hash(network: &mut NetworkService, db: &Database, hash: H256) -> Result<()> {
    if db.cas().contains(hash) {
        return Ok(());
    }

    let data = network.bitswap_fetch_hash(hash).await;
    let db_hash = db.cas().write(&data);
    ensure!(
        hash == db_hash,
        "bitswap returned data with unexpected hash"
    );
    Ok(())
}

async fn sync_compact_mb(
    network: &mut NetworkService,
    db: &Database,
    mb_hash: H256,
) -> Result<CompactMb> {
    if let Some(compact) = db.mb_compact_block(mb_hash) {
        return Ok(compact);
    }

    let compact = network
        .fetch_compact_mb(mb_hash)
        .await
        .with_context(|| format!("compact MB {mb_hash} is unavailable from network peers"))?;
    db.set_mb_compact_block(mb_hash, compact);
    sync_hash(network, db, compact.transactions_hash).await?;
    Ok(compact)
}

async fn find_latest_computed_mb(
    network: &mut NetworkService,
    db: &Database,
    mut mb_hash: H256,
) -> Result<(H256, MbMeta)> {
    loop {
        let compact = sync_compact_mb(network, db, mb_hash).await?;
        let meta = network.fetch_mb_meta(mb_hash).await;
        db.mutate_mb_meta(mb_hash, |stored| *stored = meta.clone());

        if meta.computed {
            return Ok((mb_hash, meta));
        }

        log::warn!(
            "Committed MB {mb_hash} is not computed on the serving peer, trying its parent {}",
            compact.parent
        );
        ensure!(
            !compact.parent.is_zero(),
            "no computed committed MB is available from the serving peer"
        );
        mb_hash = compact.parent;
    }
}

async fn find_syncable_committed_mb(
    network: &mut NetworkService,
    db: &Database,
    committed_mbs: Vec<H256>,
) -> Result<Option<(H256, MbMeta)>> {
    let mut last_error = None;
    for mb_hash in committed_mbs {
        match timeout(
            Duration::from_secs(6),
            find_latest_computed_mb(network, db, mb_hash),
        )
        .await
        {
            Ok(Ok(found)) => return Ok(Some(found)),
            Ok(Err(error)) => {
                log::warn!("Committed MB {mb_hash} is not syncable: {error:?}");
                last_error = Some(error);
            }
            Err(_) => {
                log::warn!("Timed out while checking whether committed MB {mb_hash} is syncable");
            }
        }
    }

    if let Some(error) = last_error {
        return Err(error).context("failed to find a syncable committed MB");
    }

    Ok(None)
}

async fn sync_latest_mb(
    network: &mut NetworkService,
    db: &Database,
    mb_hash: H256,
    code_ids: &BTreeSet<CodeId>,
) -> Result<(MbMeta, ProgramStates)> {
    let compact = sync_compact_mb(network, db, mb_hash).await?;
    sync_hash(network, db, compact.transactions_hash).await?;

    let meta = network.fetch_mb_meta(mb_hash).await;
    ensure!(
        meta.computed,
        "latest committed MB {mb_hash} is not computed on the serving peer"
    );
    db.mutate_mb_meta(mb_hash, |stored| *stored = meta.clone());

    let program_states = match db.mb_program_states(mb_hash) {
        Some(program_states) => program_states,
        None => {
            let program_states = network
                .fetch_mb_program_states(mb_hash)
                .await
                .with_context(|| {
                    format!("program states for MB {mb_hash} are unavailable from network peers")
                })?;
            db.set_mb_program_states(mb_hash, program_states.clone());
            program_states
        }
    };

    let outcome = match db.mb_outcome(mb_hash) {
        Some(outcome) => outcome,
        None => {
            let outcome = network.fetch_mb_outcome(mb_hash).await.with_context(|| {
                format!("outcome for MB {mb_hash} is unavailable from network peers")
            })?;
            db.set_mb_outcome(mb_hash, outcome.clone());
            outcome
        }
    };

    let schedule = match db.mb_schedule(mb_hash) {
        Some(schedule) => schedule,
        None => {
            let schedule = network.fetch_mb_schedule(mb_hash).await.with_context(|| {
                format!("schedule for MB {mb_hash} is unavailable from network peers")
            })?;
            db.set_mb_schedule(mb_hash, schedule.clone());
            schedule
        }
    };

    let mut manager = RequestManager::new(db.clone());
    for &code_id in code_ids {
        manager.add(code_id.into(), RequestMetadata::Data);
    }

    ethexe_db::visitor::walk(
        &mut manager,
        MbProgramStatesNode {
            mb_hash,
            mb_program_states: program_states.clone(),
        },
    );
    ethexe_db::visitor::walk(
        &mut manager,
        MbOutcomeNode {
            mb_hash,
            mb_outcome: outcome,
        },
    );
    ethexe_db::visitor::walk(
        &mut manager,
        MbScheduleNode {
            mb_hash,
            mb_schedule: schedule,
        },
    );

    while let Some(responses) = manager.request(network).await? {
        let (completed, pending) = manager.stats();
        log::info!("[{completed:>05} / {pending:>05}] Getting network data");

        for (metadata, data) in responses {
            match metadata {
                RequestMetadata::ProgramState => {
                    let state: ProgramState =
                        Decode::decode(&mut &data[..]).expect("bitswap must validate data");
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
                RequestMetadata::Data => {}
            }
        }
    }

    log::info!("Network data getting is done");
    Ok((meta, program_states))
}

async fn instrument_codes(
    compute: &mut ComputeService,
    db: &Database,
    mut code_ids: BTreeSet<CodeId>,
) -> Result<()> {
    if code_ids.is_empty() {
        log::info!("No codes to instrument. Skipping...");
        return Ok(());
    }

    log::info!("Instrument {} codes", code_ids.len());

    for &code_id in &code_ids {
        let original_code = db
            .original_code(code_id)
            .with_context(|| format!("code {code_id:?} was not fetched from the network"))?;
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
    let latest_block = observer
        .block_loader()
        .load_simple(BlockId::latest())
        .await
        .context("failed to get latest block")?
        .hash;

    let Some(EventData {
        latest_committed_batch,
        committed_mbs,
        latest_committed_eb,
    }) = EventData::collect(&block_loader, db, finalized_block)
        .await?
        .or(if latest_block == finalized_block {
            None
        } else {
            let latest_event_data = EventData::collect(&block_loader, db, latest_block).await?;
            if latest_event_data.is_some() {
                log::warn!("No finalized committed MB found; trying latest block candidates");
            }
            latest_event_data
        })
    else {
        log::warn!("No committed MB found. Skipping fast synchronization...");
        return Ok(());
    };

    let mut latest_syncable_mb = find_syncable_committed_mb(network, db, committed_mbs).await?;
    if latest_syncable_mb.is_none()
        && latest_block != finalized_block
        && let Some(EventData { committed_mbs, .. }) =
            EventData::collect(&block_loader, db, latest_block).await?
    {
        log::warn!("No finalized committed MB is syncable; trying latest block candidates");
        latest_syncable_mb = find_syncable_committed_mb(network, db, committed_mbs).await?;
    }
    let Some((latest_committed_mb, mb_meta)) = latest_syncable_mb else {
        bail!("failed to find a syncable committed MB");
    };

    let latest_committed_eb = (!mb_meta.last_advanced_eb.is_zero())
        .then_some(mb_meta.last_advanced_eb)
        .or(latest_committed_eb)
        .unwrap_or(genesis_block_hash);
    let BlockData {
        hash: block_hash,
        header,
        events,
    } = block_loader.load(latest_committed_eb, None).await?;

    let code_ids = collect_code_ids(observer, genesis_block.header.height, header.height).await?;
    let program_code_ids =
        collect_program_code_ids(observer, genesis_block.header.height, header.height).await?;

    for (program_id, code_id) in program_code_ids {
        db.set_program_code_id(program_id, code_id);
    }

    sync_latest_mb(network, db, latest_committed_mb, &code_ids).await?;

    instrument_codes(compute, db, code_ids).await?;

    set_tx_pool_data_requirement(db, &block_loader, header.height).await?;

    let latest_era_with_committed_validators =
        latest_era_with_committed_validators(db, &observer.router_query(), block_hash).await?;

    ethexe_common::setup_block_in_db(
        db,
        block_hash,
        PreparedBlockData {
            header,
            events,
            latest_era_with_committed_validators,
            codes_queue: Default::default(),
            last_committed_batch: latest_committed_batch,
            last_committed_mb: latest_committed_mb,
            last_committed_eb: latest_committed_eb,
        },
    );

    db.globals_mutate(|globals| {
        globals.start_block_hash = block_hash;
        globals.latest_synced_eb = SimpleBlockData {
            hash: block_hash,
            header,
        };
        globals.latest_prepared_eb_hash = block_hash;
        globals.latest_finalized_mb_hash = latest_committed_mb;
        globals.latest_computed_mb_hash = latest_committed_mb;
    });

    log::info!(
        "Fast synchronization done: synced to {block_hash:?}, height {:?}, MB {latest_committed_mb}",
        header.height
    );

    #[cfg(test)]
    sender
        .send(crate::tests::utils::TestingEvent::FastSyncDone(block_hash))
        .await;

    Ok(())
}
