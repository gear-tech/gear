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
use anyhow::{ensure, Context, Result};
use ethexe_common::{
    db::{BlockHeader, BlockMetaStorage, CodesStorage, OnChainStorage},
    events::{BlockEvent, MirrorEvent, RouterEvent},
    gear::CodeCommitment,
};
use ethexe_compute::{ComputeEvent, ComputeService};
use ethexe_db::Database;
use ethexe_ethereum::mirror::MirrorQuery;
use ethexe_network::{db_sync, NetworkEvent, NetworkService};
use ethexe_observer::ObserverService;
use ethexe_runtime_common::{
    state::{
        ActiveProgram, DispatchStash, Expiring, Mailbox, MaybeHashOf, MemoryPages,
        MemoryPagesRegion, MessageQueue, PayloadLookup, Program, ProgramState, UserMailbox,
        Waitlist,
    },
    ScheduleRestorer,
};
use ethexe_signer::Address;
use futures::StreamExt;
use gprimitives::{ActorId, CodeId, H256};
use parity_scale_codec::Decode;
use std::{
    collections::{BTreeMap, HashMap, HashSet, VecDeque},
    iter,
};

struct EventData {
    /// Latest committed on the chain and not computed local block
    latest_committed_block: H256,
    /// Previous committed block
    previous_committed_block: Option<H256>,
    /// Events between the latest and previous blocks (inclusively) from top to bottom
    events_in_between: Vec<BlockEvent>,
}

impl EventData {
    async fn collect(
        observer: &mut ObserverService,
        db: &Database,
        highest_block: H256,
    ) -> Result<Option<Self>> {
        let mut previous_committed_block = None;
        let mut latest_committed_block = None;
        let mut events_in_between = Vec::new();

        let mut block = highest_block;
        while !db.block_computed(block) {
            let (header, events) = match db.block_events(block) {
                Some(events) => {
                    let header = db
                        .block_header(block)
                        .expect("observer must fulfill database");
                    (header, events)
                }
                None => {
                    let data = observer.load_block_data(block).await?;
                    (data.header, data.events)
                }
            };

            // NOTE: logic relies on events in order as they are emitted on Ethereum
            for event in events.iter().rev() {
                if let BlockEvent::Router(RouterEvent::BlockCommitted { hash }) = event {
                    if latest_committed_block.is_none() {
                        latest_committed_block = Some(*hash);
                    } else {
                        previous_committed_block = Some(*hash);
                    }
                }
            }

            if latest_committed_block.is_some() || previous_committed_block.is_some() {
                events_in_between.extend(events.into_iter().rev());
            }

            if previous_committed_block.is_some() {
                break;
            }

            let parent = header.parent_hash;
            block = parent;
        }

        let Some(latest_committed_block) = latest_committed_block else {
            return Ok(None);
        };

        // TODO: uncomment
        // #[cfg(debug_assertions)]
        // if let Some(previous_committed_block) = previous_committed_block {
        //     let latest_block_header = OnChainStorage::block_header(db, latest_committed_block)
        //         .expect("observer must fulfill database");
        //     let previous_block_header = OnChainStorage::block_header(db, previous_committed_block)
        //         .expect("observer must fulfill database");
        //     assert!(previous_block_header.height < latest_block_header.height);
        // }

        Ok(Some(Self {
            latest_committed_block,
            previous_committed_block,
            events_in_between,
        }))
    }
}

async fn collect_program_code_ids(
    observer: &mut ObserverService,
    network: &mut NetworkService,
    latest_committed_block: H256,
) -> Result<BTreeMap<ActorId, CodeId>> {
    let result = net_fetch(
        network,
        db_sync::Request::ProgramIdsAt(latest_committed_block),
    )
    .await;
    let program_ids = match result {
        Ok(db_sync::Response::ProgramIdsAt(block, program_ids)) => {
            debug_assert_eq!(block, latest_committed_block);
            program_ids
        }
        Ok(db_sync::Response::Hashes(_)) => unreachable!(),
        Err(e) => todo!("{e}"),
    };

    let router_query = observer.router_query();
    let code_ids = router_query
        .programs_code_ids(program_ids.iter().copied())
        .await?;

    let program_code_ids = iter::zip(program_ids, code_ids).collect();
    Ok(program_code_ids)
}

async fn collect_program_states(
    observer: &mut ObserverService,
    latest_committed_block: H256,
    program_code_ids: &BTreeMap<ActorId, CodeId>,
    events_in_between: Vec<BlockEvent>,
) -> Result<BTreeMap<ActorId, H256>> {
    let mut program_states = BTreeMap::new();
    let mut uninitialized_mirrors = Vec::new();

    let provider = observer.provider();

    for &actor_id in program_code_ids.keys() {
        let mirror = Address::try_from(actor_id).expect("invalid actor id");
        let mirror = MirrorQuery::new(provider.clone(), mirror);

        let state_hash = mirror
            .state_hash_at(latest_committed_block)
            .await
            .with_context(|| {
                format!("Failed to get state hash for actor {actor_id} at block {latest_committed_block}",)
            })?;

        if state_hash.is_zero() {
            uninitialized_mirrors.push(actor_id);
            continue;
        }

        program_states.insert(actor_id, state_hash);
    }

    // NOTE: iteration goes from bottom to top
    for event in events_in_between.into_iter().rev() {
        if let BlockEvent::Mirror {
            actor_id,
            event: MirrorEvent::StateChanged { state_hash },
        } = event
        {
            // the latest committed block and its `BlockCommitted` event can be a few blocks apart,
            // so some mirror state changes must be obtained from Ethereum events
            program_states.insert(actor_id, state_hash);
        }
    }

    for actor_id in uninitialized_mirrors {
        ensure!(
            program_states.contains_key(&actor_id),
            "mirror {actor_id} expected to be initialized, but it is not"
        );
    }

    Ok(program_states)
}

async fn net_fetch(
    network: &mut NetworkService,
    request: db_sync::Request,
) -> Result<db_sync::Response, db_sync::RequestFailure> {
    let request_id = network.db_sync().request(request);

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
        Ok(response) => Ok(response),
        Err((request, err)) => {
            network.db_sync().retry(request);
            Err(err)
        }
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
            let request = pending_network_requests.keys().copied().collect();
            let result = net_fetch(network, db_sync::Request::Hashes(request)).await;

            match result {
                Ok(response) => {
                    self.handle_response(pending_network_requests, response, db);
                }
                Err(err) => {
                    self.pending_requests.extend(pending_network_requests);
                    // TODO: print request ID
                    log::warn!("Request failed: {err}. Retrying...");
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
        let db_sync::Response::Hashes(data) = response else {
            unreachable!("`db-sync` must return `Hashes` response");
        };

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

async fn sync_from_network(
    network: &mut NetworkService,
    db: &Database,
    program_code_ids: &BTreeMap<ActorId, CodeId>,
    program_states: &BTreeMap<ActorId, H256>,
) {
    let add_payload = |manager: &mut RequestManager, payload: &PayloadLookup| match payload {
        PayloadLookup::Direct(_) => {}
        PayloadLookup::Stored(hash) => {
            manager.add(hash.hash(), RequestMetadata::Data);
        }
    };

    let mut manager = RequestManager::default();

    for &state in program_states.values() {
        manager.add(state, RequestMetadata::ProgramState);
    }

    for &code_id in program_code_ids.values() {
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
                        queue_hash,
                        waitlist_hash,
                        stash_hash,
                        mailbox_hash,
                        balance: _,
                        executable_balance: _,
                    } = &state;

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

                    if let Some(queue_hash) = queue_hash.hash() {
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
}

async fn instrument_codes(
    db: &Database,
    compute: &mut ComputeService,
    program_code_ids: &BTreeMap<ActorId, CodeId>,
) -> Result<()> {
    /// codes we instrument had already been processed by gear.exe,
    /// so generated code commitments are never going to be submitted,
    /// so we just pass placeholder value for their timestamp
    const TIMESTAMP: u64 = u64::MAX;

    let mut code_ids: HashSet<CodeId> = program_code_ids.values().copied().collect();
    if code_ids.is_empty() {
        log::info!("No codes to instrument. Skipping...");
        return Ok(());
    }

    log::info!("Instrument {} codes", code_ids.len());

    for &code_id in &code_ids {
        let original_code = db
            .original_code(code_id)
            .expect("`sync_from_network` must fulfill database");
        compute.receive_code(code_id, TIMESTAMP, original_code);
    }

    while let Some(event) = compute.next().await {
        if let ComputeEvent::CodeProcessed(CodeCommitment { id, timestamp, .. }) = event? {
            debug_assert_eq!(timestamp, TIMESTAMP);
            code_ids.remove(&id);
            if code_ids.is_empty() {
                break;
            }
        }
    }

    log::info!("Instrumentation done");
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
        latest_committed_block,
        previous_committed_block,
        events_in_between,
    }) = EventData::collect(observer, db, finalized_block).await?
    else {
        log::warn!("No any committed block found. Skipping fast synchronization...");
        return Ok(());
    };

    let program_code_ids =
        collect_program_code_ids(observer, network, latest_committed_block).await?;

    let program_states = collect_program_states(
        observer,
        latest_committed_block,
        &program_code_ids,
        events_in_between,
    )
    .await?;

    sync_from_network(network, db, &program_code_ids, &program_states).await;

    instrument_codes(db, compute, &program_code_ids).await?;

    let latest_block_header = observer
        .provider()
        .get_block_by_hash(latest_committed_block.0.into())
        .await
        .context("failed to get commited block info from Ethereum")?
        .with_context(|| {
            format!("Latest commited block not found by hash: {latest_committed_block}")
        })?;
    let latest_block_header = BlockHeader {
        height: latest_block_header.header.number as u32,
        timestamp: latest_block_header.header.timestamp,
        parent_hash: H256(latest_block_header.header.parent_hash.0),
    };

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

    // set by observer service normally
    db.set_block_is_synced(latest_committed_block);
    db.set_latest_synced_block_height(latest_block_header.height);

    // set by compute service normally
    db.set_block_computed(latest_committed_block);
    db.set_latest_computed_block(latest_committed_block, latest_block_header);

    log::info!("Fast synchronization done");

    #[cfg(test)]
    sender
        .send(crate::tests::utils::TestableEvent::FastSyncDone(
            latest_committed_block,
        ))
        .expect("failed to broadcast fast sync done event");

    Ok(())
}
