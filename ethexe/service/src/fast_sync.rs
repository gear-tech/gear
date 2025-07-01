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

use crate::Service;
use alloy::{eips::BlockId, providers::Provider};
use anyhow::{anyhow, Context, Result};
use ethexe_blob_loader::{BlobData, BlobLoaderEvent, BlobLoaderService};
use ethexe_common::{
    db::{
        BlockMetaStorageRead, BlockMetaStorageWrite, CodesStorageRead, CodesStorageWrite,
        OnChainStorageRead,
    },
    events::{BlockEvent, MirrorEvent, RouterEvent},
    ProgramStates, StateHashWithQueueSize,
};
use ethexe_compute::{ComputeEvent, ComputeService};
use ethexe_db::Database;
use ethexe_network::{db_sync, NetworkEvent, NetworkService};
use ethexe_observer::{ObserverEvent, ObserverService};
use ethexe_runtime_common::{
    state::{
        ActiveProgram, DispatchStash, Expiring, Mailbox, MaybeHashOf, MemoryPages,
        MemoryPagesRegion, MessageQueue, PayloadLookup, Program, ProgramState, UserMailbox,
        Waitlist,
    },
    ScheduleRestorer,
};
use futures::StreamExt;
use gprimitives::{ActorId, CodeId, H256};
use parity_scale_codec::Decode;
use std::collections::{BTreeMap, HashMap, HashSet, VecDeque};

struct EventData {
    program_states: BTreeMap<ActorId, H256>,
    program_code_ids: Vec<(ActorId, CodeId)>,
    needs_instrumentation_codes: HashSet<CodeId>,
    /// Latest committed on the chain and not computed local block
    latest_committed_block: H256,
    /// Previous committed block
    previous_committed_block: Option<H256>,
}

impl EventData {
    async fn collect(db: &Database, highest_block: H256) -> Result<Option<Self>> {
        let mut program_states = BTreeMap::new();
        let mut program_code_ids = Vec::new();
        let mut needs_instrumentation_codes = HashSet::new();
        let mut previous_committed_block = None;
        let mut latest_committed_block = None;

        let mut block = highest_block;
        while !db.block_meta(block).computed {
            let events = db
                .block_events(block)
                .ok_or_else(|| anyhow!("no events found for block {block}"))?;

            // NOTE: logic relies on events in order as they are emitted on Ethereum
            for event in events.into_iter().rev() {
                if let BlockEvent::Router(RouterEvent::CodeGotValidated {
                    code_id,
                    valid: true,
                }) = event
                {
                    if !db.instrumented_code_exists(ethexe_runtime::VERSION, code_id) {
                        needs_instrumentation_codes.insert(code_id);
                    }
                    continue;
                }

                if latest_committed_block.is_none() {
                    if let BlockEvent::Router(RouterEvent::BlockCommitted { hash }) = event {
                        latest_committed_block = Some(hash);
                    }
                    // we don't collect any further info until the latest committed block is known
                    continue;
                }

                match event {
                    BlockEvent::Mirror {
                        actor_id,
                        event: MirrorEvent::StateChanged { state_hash },
                    } => {
                        program_states.entry(actor_id).or_insert(state_hash);
                    }
                    BlockEvent::Router(RouterEvent::BlockCommitted { hash }) => {
                        previous_committed_block.get_or_insert(hash);
                    }
                    BlockEvent::Router(RouterEvent::ProgramCreated { actor_id, code_id }) => {
                        program_code_ids.push((actor_id, code_id));
                    }
                    _ => {}
                }
            }

            let header = db
                .block_header(block)
                .ok_or_else(|| anyhow!("header not found for synced block {block}"))?;
            let parent = header.parent_hash;
            block = parent;
        }

        let Some(latest_committed_block) = latest_committed_block else {
            return Ok(None);
        };

        // recover data we haven't seen in events by the latest computed block
        // NOTE: we use `block` instead of `db.latest_computed_block()` so
        // possible reorganization have no effect
        let computed_program_states: BTreeMap<_, _> = db
            .block_program_states(block)
            .context("program states of latest computed block not found")?
            .into_iter()
            .map(|(program_id, state)| (program_id, state.hash))
            .collect();

        for (program_id, state) in computed_program_states {
            program_states.entry(program_id).or_insert(state);
        }

        #[cfg(debug_assertions)]
        if let Some(previous_committed_block) = previous_committed_block {
            let latest_block_header = db
                .block_header(latest_committed_block)
                .expect("observer must fulfill database");
            let previous_block_header = db
                .block_header(previous_committed_block)
                .expect("observer must fulfill database");
            assert!(previous_block_header.height < latest_block_header.height);
        }

        Ok(Some(Self {
            program_states,
            program_code_ids,
            needs_instrumentation_codes,
            latest_committed_block,
            previous_committed_block,
        }))
    }
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

#[derive(Debug, Default)]
struct RequestManager {
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
    fn add(&mut self, hash: H256, metadata: RequestMetadata) {
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
        db: &Database,
    ) -> Option<Vec<(RequestMetadata, Vec<u8>)>> {
        let pending_network_requests = self.handle_pending_requests(db);

        if !pending_network_requests.is_empty() {
            let request = pending_network_requests.keys().copied().collect();
            let request_id = network.db_sync().request(db_sync::Request(request));

            let result = loop {
                let event = network
                    .next()
                    .await
                    .expect("network service stream is infinite");

                if let NetworkEvent::DbResponse {
                    request_id: rid,
                    result,
                } = event
                {
                    debug_assert_eq!(rid, request_id, "unknown request id");
                    break result;
                }
            };

            match result {
                Ok(response) => {
                    self.handle_response(pending_network_requests, response, db);
                }
                Err((request, err)) => {
                    network.db_sync().retry(request);
                    self.pending_requests.extend(pending_network_requests);
                    log::warn!("{request_id:?} failed: {err}. Retrying...");
                }
            }
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

    fn handle_pending_requests(&mut self, db: &Database) -> HashMap<H256, RequestMetadata> {
        let mut pending_requests = HashMap::new();
        for (hash, metadata) in self.pending_requests.drain() {
            if metadata.is_data() && db.contains_hash(hash) {
                self.total_completed_requests += 1;
                continue;
            }

            if let Some(data) = db.read_by_hash(hash) {
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
        response: db_sync::Response,
        db: &Database,
    ) {
        let db_sync::Response(data) = response;

        for (hash, data) in data {
            let metadata = pending_network_requests
                .remove(&hash)
                .expect("unknown pending request");

            let db_hash = db.write_hash(&data);
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

impl Drop for RequestManager {
    fn drop(&mut self) {
        #[cfg(debug_assertions)]
        {
            let Self {
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

async fn sync_finalized_head(observer: &mut ObserverService) -> Result<H256> {
    let highest_block = observer
        .provider()
        // we get finalized block to avoid block reorganization
        // because we restore the database only for the latest block of a chain,
        // and thus the reorganization can lead us to an empty block
        .get_block(BlockId::finalized())
        .await
        .context("failed to get latest block")?
        .expect("latest block always exist");
    let highest_block = H256(highest_block.header.hash.0);

    log::info!("Syncing chain head {highest_block}");
    observer.force_sync_block(highest_block).await?;

    while let Some(event) = observer.next().await {
        match event? {
            ObserverEvent::Block(_) => {}
            ObserverEvent::BlockSynced(synced_block) => {
                debug_assert_eq!(highest_block, synced_block.block_hash);
                break;
            }
        }
    }

    Ok(highest_block)
}

async fn sync_from_network(
    network: &mut NetworkService,
    db: &Database,
    program_states: BTreeMap<ActorId, H256>,
) -> ProgramStates {
    let add_payload = |manager: &mut RequestManager, payload: &PayloadLookup| match payload {
        PayloadLookup::Direct(_) => {}
        PayloadLookup::Stored(hash) => {
            manager.add(hash.hash(), RequestMetadata::Data);
        }
    };

    let mut restored_cached_queue_sizes = BTreeMap::new();

    let mut manager = RequestManager::default();
    for &state in program_states.values() {
        manager.add(state, RequestMetadata::ProgramState);
    }

    loop {
        let (completed, pending) = manager.stats();
        log::info!("[{completed:>05} / {pending:>05}] Getting network data");

        let Some(responses) = manager.request(network, db).await else {
            break;
        };

        for (metadata, data) in responses {
            match metadata {
                RequestMetadata::ProgramState => {
                    let state: ProgramState =
                        Decode::decode(&mut &data[..]).expect("`db-sync` must validate data");

                    let ProgramState {
                        program,
                        queue,
                        waitlist_hash,
                        stash_hash,
                        mailbox_hash,
                        balance: _,
                        executable_balance: _,
                    } = &state;

                    // Save restored cached queue sizes
                    let program_state_hash = ethexe_db::hash(&data);
                    restored_cached_queue_sizes.insert(program_state_hash, queue.cached_queue_size);

                    if let Program::Active(ActiveProgram {
                        allocations_hash,
                        pages_hash,
                        memory_infix: _,
                        initialized: _,
                    }) = program
                    {
                        if let Some(allocations_hash) = allocations_hash.hash() {
                            manager.add(allocations_hash, RequestMetadata::Data);
                        }
                        if let Some(pages_hash) = pages_hash.hash() {
                            manager.add(pages_hash, RequestMetadata::MemoryPages);
                        }
                    }

                    if let Some(queue_hash) = queue.hash.hash() {
                        manager.add(queue_hash, RequestMetadata::MessageQueue);
                    }
                    if let Some(waitlist_hash) = waitlist_hash.hash() {
                        manager.add(waitlist_hash, RequestMetadata::Waitlist);
                    }
                    if let Some(mailbox_hash) = mailbox_hash.hash() {
                        manager.add(mailbox_hash, RequestMetadata::Mailbox);
                    }
                    if let Some(stash_hash) = stash_hash.hash() {
                        manager.add(stash_hash, RequestMetadata::DispatchStash);
                    }
                }
                RequestMetadata::MemoryPages => {
                    let memory_pages: MemoryPages =
                        Decode::decode(&mut &data[..]).expect("`db-sync` must validate data");

                    for pages_region_hash in memory_pages
                        .to_inner()
                        .into_iter()
                        .flat_map(MaybeHashOf::hash)
                    {
                        manager.add(pages_region_hash, RequestMetadata::MemoryPagesRegion);
                    }
                }
                RequestMetadata::MemoryPagesRegion => {
                    let pages_region: MemoryPagesRegion =
                        Decode::decode(&mut &data[..]).expect("`db-sync` must validate data");

                    for page_buf_hash in pages_region.as_inner().values().map(|hash| hash.hash()) {
                        manager.add(page_buf_hash, RequestMetadata::Data);
                    }
                }
                RequestMetadata::MessageQueue => {
                    let message_queue: MessageQueue =
                        Decode::decode(&mut &data[..]).expect("`db-sync` must validate data");
                    for dispatch in message_queue.as_ref() {
                        add_payload(&mut manager, &dispatch.payload);
                    }
                }
                RequestMetadata::Waitlist => {
                    let waitlist: Waitlist =
                        Decode::decode(&mut &data[..]).expect("`db-sync` must validate data");
                    for Expiring {
                        value: dispatch,
                        expiry: _,
                    } in waitlist.as_ref().values()
                    {
                        add_payload(&mut manager, &dispatch.payload);
                    }
                }
                RequestMetadata::Mailbox => {
                    let mailbox: Mailbox =
                        Decode::decode(&mut &data[..]).expect("`db-sync` must validate data");
                    for user_mailbox in mailbox.as_ref().values() {
                        manager.add(user_mailbox.hash(), RequestMetadata::UserMailbox);
                    }
                }
                RequestMetadata::UserMailbox => {
                    let user_mailbox: UserMailbox =
                        Decode::decode(&mut &data[..]).expect("`db-sync` must validate data");
                    for Expiring {
                        value: msg,
                        expiry: _,
                    } in user_mailbox.as_ref().values()
                    {
                        add_payload(&mut manager, &msg.payload);
                    }
                }
                RequestMetadata::DispatchStash => {
                    let dispatch_stash: DispatchStash =
                        Decode::decode(&mut &data[..]).expect("`db-sync` must validate data");
                    for Expiring {
                        value: (dispatch, _user_id),
                        expiry: _,
                    } in dispatch_stash.as_ref().values()
                    {
                        add_payload(&mut manager, &dispatch.payload);
                    }
                }
                RequestMetadata::Data => {}
            }
        }
    }

    log::info!("Network data getting is done");

    // Enrich program states with cached queue size
    program_states
        .into_iter()
        .map(|(program_id, hash)| {
            let cached_queue_size = *restored_cached_queue_sizes
                .get(&hash)
                .expect("program state cached queue size must be restored");
            (
                program_id,
                StateHashWithQueueSize {
                    hash,
                    cached_queue_size,
                },
            )
        })
        .collect()
}

async fn prepare_codes(
    compute: &mut ComputeService,
    blobs_loader: &mut Box<dyn BlobLoaderService>,
    synced_block: H256,
    mut code_ids: HashSet<CodeId>,
) -> Result<()> {
    if code_ids.is_empty() {
        log::info!("No codes to instrument. Skipping...");
        return Ok(());
    }

    compute.prepare_block(synced_block);
    match compute.next().await {
        Some(Ok(ComputeEvent::RequestLoadCodes(codes))) => blobs_loader.load_codes(codes, None)?,
        Some(Ok(event)) => {
            return Err(anyhow!(
                "expect codes to load, but got another event: {event:?}"
            ))
        }
        Some(Err(e)) => return Err(anyhow!("expect codes to load, but got err: {e:?}")),
        None => return Err(anyhow!("expect codes to load, but got None")),
    };

    log::info!("Instrument {} codes", code_ids.len());

    let mut wait_load_codes = code_ids.clone();
    while !wait_load_codes.is_empty() {
        match blobs_loader.next().await {
            Some(Ok(BlobLoaderEvent::BlobLoaded(BlobData {
                code_id,
                timestamp,
                code,
            }))) => {
                wait_load_codes.remove(&code_id);
                compute.process_code(code_id, timestamp, code);
            }
            Some(Err(e)) => {
                return Err(anyhow!("expect BlobLoaded, but got err: {e:?}"));
            }
            None => {
                return Err(anyhow!("expect BlobLoaded, but got None"));
            }
        }
    }

    while let Some(event) = compute.next().await {
        if let ComputeEvent::CodeProcessed(id) = event? {
            code_ids.remove(&id);
            if code_ids.is_empty() {
                break;
            }
        }
    }

    log::info!("Codes preparation done");
    Ok(())
}

pub(crate) async fn sync(service: &mut Service) -> Result<()> {
    let Service {
        observer,
        blob_loader,
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

    let finalized_block = sync_finalized_head(observer).await?;
    let Some(EventData {
        program_states,
        program_code_ids,
        needs_instrumentation_codes,
        latest_committed_block,
        previous_committed_block,
    }) = EventData::collect(db, finalized_block).await?
    else {
        log::warn!("No any committed block found. Skipping fast synchronization...");
        return Ok(());
    };

    prepare_codes(
        compute,
        blob_loader,
        finalized_block,
        needs_instrumentation_codes,
    )
    .await?;

    let latest_block_header = db
        .block_header(latest_committed_block)
        .expect("observer must fulfill database");

    let program_states = sync_from_network(network, db, program_states).await;

    let schedule =
        ScheduleRestorer::from_storage(db, &program_states, latest_block_header.height)?.restore();

    for (program_id, code_id) in program_code_ids {
        db.set_program_code_id(program_id, code_id);
    }

    // NOTE: there is no invariant that fast sync should recover queues
    db.set_block_commitment_queue(latest_committed_block, VecDeque::new());
    db.set_block_codes_queue(latest_committed_block, VecDeque::new());

    db.set_block_program_states(latest_committed_block, program_states);
    db.set_block_schedule(latest_committed_block, schedule);
    unsafe {
        db.set_non_empty_block_outcome(latest_committed_block);
    }
    db.set_previous_not_empty_block(
        latest_committed_block,
        previous_committed_block.unwrap_or_else(H256::zero),
    );
    db.mutate_block_meta(latest_committed_block, |meta| meta.computed = true);
    db.set_latest_computed_block(latest_committed_block, latest_block_header);

    log::info!("Fast synchronization done");

    #[cfg(test)]
    sender
        .send(crate::tests::utils::TestingEvent::FastSyncDone(
            latest_committed_block,
        ))
        .expect("failed to broadcast fast sync done event");

    Ok(())
}
