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
use anyhow::{Context, Result};
use ethexe_common::{
    Address, BlockData, CodeAndIdUnchecked, Digest, ProgramStates, StateHashWithQueueSize,
    db::{
        BlockMetaStorageRead, BlockMetaStorageWrite, CodesStorageRead, CodesStorageWrite,
        OnChainStorageRead, OnChainStorageWrite,
    },
    events::{BlockEvent, RouterEvent},
};
use ethexe_compute::ComputeService;
use ethexe_db::Database;
use ethexe_ethereum::mirror::MirrorQuery;
use ethexe_network::{NetworkEvent, NetworkService, db_sync};
use ethexe_observer::ObserverService;
use ethexe_runtime_common::{
    ScheduleRestorer,
    state::{
        ActiveProgram, DispatchStash, Expiring, Mailbox, MaybeHashOf, MemoryPages,
        MemoryPagesRegion, MessageQueue, PayloadLookup, Program, ProgramState, UserMailbox,
        Waitlist,
    },
};
use futures::StreamExt;
use gprimitives::{ActorId, CodeId, H256};
use parity_scale_codec::Decode;
use std::collections::{BTreeMap, BTreeSet, HashMap};

struct EventData {
    /// Latest committed on the chain and not computed local batch
    latest_committed_batch: Digest,
    /// Latest committed on the chain and not computed local block
    latest_committed_block: BlockData,
}

impl EventData {
    async fn get_block_data(
        observer: &mut ObserverService,
        db: &Database,
        block: H256,
    ) -> Result<BlockData> {
        if let (Some(header), Some(events)) = (db.block_header(block), db.block_events(block)) {
            Ok(BlockData {
                hash: block,
                header,
                events,
            })
        } else {
            let data = observer.load_block_data(block).await?;
            db.set_block_header(block, data.header.clone());
            db.set_block_events(block, &data.events);
            Ok(data)
        }
    }

    /// Collects metadata regarding the latest committed batch, block, and the previous committed block
    /// for a given blockchain observer and database.
    async fn collect(
        observer: &mut ObserverService,
        db: &Database,
        highest_block: H256,
    ) -> Result<Option<Self>> {
        let mut latest_committed: Option<(Digest, Option<H256>)> = None;

        let mut block = highest_block;
        'computed: while !db.block_meta(block).computed {
            let block_data = Self::get_block_data(observer, db, block).await?;

            // NOTE: logic relies on events in order as they are emitted on Ethereum
            for event in block_data.events.into_iter().rev() {
                match event {
                    BlockEvent::Router(RouterEvent::BatchCommitted { digest })
                        if latest_committed.is_none() =>
                    {
                        latest_committed = Some((digest, None));
                    }
                    BlockEvent::Router(RouterEvent::HeadCommitted(head)) => {
                        let Some((_, latest_committed_head)) = latest_committed.as_mut() else {
                            anyhow::bail!(
                                "Inconsistent block events: head commitment before batch commitment"
                            );
                        };
                        assert!(
                            latest_committed_head.is_none(),
                            "The loop have to be broken after the first head commitment"
                        );
                        *latest_committed_head = Some(head);

                        break 'computed;
                    }
                    _ => {}
                }
            }

            block = block_data.header.parent_hash;
        }

        let Some((latest_committed_batch, Some(latest_committed_block))) = latest_committed else {
            return Ok(None);
        };

        let latest_committed_block_data =
            Self::get_block_data(observer, db, latest_committed_block).await?;

        Ok(Some(Self {
            latest_committed_batch,
            latest_committed_block: latest_committed_block_data,
        }))
    }
}

async fn net_fetch(
    network: &mut NetworkService,
    request: db_sync::Request,
) -> Result<db_sync::Response> {
    let request_id = network.db_sync().request(request);
    loop {
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
            match result {
                Ok(response) => break Ok(response),
                Err((request, err)) => {
                    log::warn!("Request {:?} failed: {err}. Retrying...", request.id());
                    network.db_sync().retry(request);
                    continue;
                }
            }
        }
    }
}

/// Сollects program code IDs for the latest committed block.
async fn collect_program_code_ids(
    observer: &mut ObserverService,
    network: &mut NetworkService,
    latest_committed_block: H256,
) -> Result<BTreeMap<ActorId, CodeId>> {
    let router_query = observer.router_query();
    let programs_count = router_query
        .programs_count_at(latest_committed_block)
        .await?;

    let response = net_fetch(
        network,
        db_sync::Request::program_ids(latest_committed_block, programs_count),
    )
    .await?;

    let program_code_ids = response.unwrap_program_ids();
    Ok(program_code_ids)
}

/// Collects a set of valid code IDs that are not yet validated in the local database.
async fn collect_code_ids(
    observer: &mut ObserverService,
    network: &mut NetworkService,
    db: &Database,
    latest_committed_block: H256,
) -> Result<BTreeSet<CodeId>> {
    let router_query = observer.router_query();
    let codes_count = router_query
        .validated_codes_count_at(latest_committed_block)
        .await?;

    let response = net_fetch(
        network,
        db_sync::Request::valid_codes(latest_committed_block, codes_count),
    )
    .await?;

    let code_ids = response
        .unwrap_valid_codes()
        .into_iter()
        .filter(|&code_id| db.code_valid(code_id).is_none())
        .collect();

    Ok(code_ids)
}

/// Collects the program states for a given set of program IDs at a specified block height.
async fn collect_program_states(
    observer: &mut ObserverService,
    at: H256,
    program_code_ids: &BTreeMap<ActorId, CodeId>,
) -> Result<BTreeMap<ActorId, H256>> {
    let mut program_states = BTreeMap::new();
    let provider = observer.provider();

    for &actor_id in program_code_ids.keys() {
        let mirror = Address::try_from(actor_id).expect("invalid actor id");
        let mirror = MirrorQuery::new(provider.clone(), mirror);

        let state_hash = mirror.state_hash_at(at).await.with_context(|| {
            format!("Failed to get state hash for actor {actor_id} at block {at}",)
        })?;

        anyhow::ensure!(
            !state_hash.is_zero(),
            "State hash is zero for actor {actor_id} at block {at}"
        );

        program_states.insert(actor_id, state_hash);
    }

    Ok(program_states)
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
        db: &Database,
    ) -> Option<Vec<(RequestMetadata, Vec<u8>)>> {
        let pending_network_requests = self.handle_pending_requests(db);

        if !pending_network_requests.is_empty() {
            let request: BTreeSet<H256> = pending_network_requests.keys().copied().collect();
            let response = net_fetch(network, db_sync::Request::hashes(request))
                .await
                .expect("no external validation required");

            self.handle_response(pending_network_requests, response, db);
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
        let data = response.unwrap_hashes();
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

    for &code_id in code_ids {
        manager.add(code_id.into(), RequestMetadata::Data);
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

/// Instruments a set of codes by delegating their processing to the `ComputeService`.
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
        .provider()
        // we get finalized block to avoid block reorganization
        // because we restore the database only for the latest block of a chain,
        // and thus the reorganization can lead us to an empty block
        .get_block(BlockId::finalized())
        .await
        .context("failed to get latest block")?
        .expect("latest block always exist");
    let finalized_block = H256(finalized_block.header.hash.0);

    let Some(EventData {
        latest_committed_batch,
        latest_committed_block:
            BlockData {
                hash: latest_committed_block,
                header: latest_block_header,
                events: latest_block_events,
            },
    }) = EventData::collect(observer, db, finalized_block).await?
    else {
        log::warn!("No any committed block found. Skipping fast synchronization...");
        return Ok(());
    };

    let code_ids = collect_code_ids(observer, network, db, latest_committed_block).await?;
    let program_code_ids =
        collect_program_code_ids(observer, network, latest_committed_block).await?;
    // we fetch program states from the finalized block
    // because actual states are at the same block as we acquired the latest committed block
    let program_states =
        collect_program_states(observer, finalized_block, &program_code_ids).await?;

    let program_states = sync_from_network(network, db, &code_ids, program_states).await;

    instrument_codes(compute, db, code_ids).await?;

    let schedule =
        ScheduleRestorer::from_storage(db, &program_states, latest_block_header.height)?.restore();

    for (program_id, code_id) in program_code_ids {
        db.set_program_code_id(program_id, code_id);
    }

    // TODO #4563: this is a temporary solution.
    // from `pre_process_genesis_for_db`
    {
        db.set_block_header(latest_committed_block, latest_block_header.clone());
        db.set_block_events(latest_committed_block, &latest_block_events);

        db.set_latest_synced_block_height(latest_block_header.height);
        db.mutate_block_meta(latest_committed_block, |meta| {
            meta.synced = true;
            meta.prepared = true;
            meta.computed = true;
            meta.last_committed_batch = Some(latest_committed_batch);
            meta.last_committed_head = Some(latest_committed_block);
        });

        // NOTE: there is no invariant that fast sync should recover queues
        db.set_block_codes_queue(latest_committed_block, Default::default());
        db.set_block_program_states(latest_committed_block, program_states);
        db.set_block_schedule(latest_committed_block, schedule);
        unsafe {
            db.set_non_empty_block_outcome(latest_committed_block);
        }

        db.set_latest_computed_block(latest_committed_block, latest_block_header);
    }

    log::info!("Fast synchronization done");

    #[cfg(test)]
    sender
        .send(crate::tests::utils::TestingEvent::FastSyncDone(
            latest_committed_block,
        ))
        .expect("failed to broadcast fast sync done event");

    Ok(())
}
