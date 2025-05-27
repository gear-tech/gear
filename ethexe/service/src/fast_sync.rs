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
    db::{BlockMetaStorage, CodesStorage, OnChainStorage},
    events::{BlockEvent, RouterEvent},
    gear::{CodeCommitment, CodeState},
    Address, BlockData,
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
use futures::StreamExt;
use gprimitives::{ActorId, CodeId, H256};
use parity_scale_codec::Decode;
use std::{
    collections::{BTreeMap, BTreeSet, HashMap},
    iter,
};

struct EventData {
    /// Latest committed on the chain and not computed local block
    latest_committed_block: BlockData,
    /// Previous committed block
    previous_committed_block: Option<H256>,
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

    async fn collect(
        observer: &mut ObserverService,
        db: &Database,
        highest_block: H256,
    ) -> Result<Option<Self>> {
        let mut previous_committed_block = None;
        let mut latest_committed_block = None;

        let mut block = highest_block;
        'computed: while !db.block_computed(block) {
            let block_data = Self::get_block_data(observer, db, block).await?;

            // NOTE: logic relies on events in order as they are emitted on Ethereum
            for event in block_data.events.into_iter().rev() {
                if latest_committed_block.is_none() {
                    if let BlockEvent::Router(RouterEvent::BlockCommitted { hash }) = event {
                        latest_committed_block = Some(hash);
                    }
                    // we don't collect any further info until the latest committed block is known
                    continue;
                }

                if let BlockEvent::Router(RouterEvent::BlockCommitted { hash }) = event {
                    previous_committed_block = Some(hash);
                    // we don't want event data of the previous committed block
                    break 'computed;
                }
            }

            let parent = block_data.header.parent_hash;
            block = parent;
        }

        let Some(latest_committed_block) = latest_committed_block else {
            return Ok(None);
        };

        let latest_committed_block =
            Self::get_block_data(observer, db, latest_committed_block).await?;

        if let Some(previous_committed_block) = previous_committed_block {
            let previous_committed_block =
                Self::get_block_data(observer, db, previous_committed_block).await?;
            debug_assert!(
                previous_committed_block.header.height < latest_committed_block.header.height
            );
        }

        Ok(Some(Self {
            latest_committed_block,
            previous_committed_block,
        }))
    }
}

async fn net_fetch<F, R>(
    network: &mut NetworkService,
    request: db_sync::Request,
    mut external_validation: F,
) -> Result<db_sync::Response>
where
    F: FnMut(db_sync::Response) -> R,
    R: Future<Output = Result<bool>>,
{
    let request_id = network.db_sync().request(request);
    loop {
        let event = network
            .next()
            .await
            .expect("network service stream is infinite");

        match event {
            NetworkEvent::DbResponse {
                request_id: rid,
                result,
            } => {
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
            NetworkEvent::DbExternalValidation {
                request_id: rid,
                response,
                sender,
            } => {
                debug_assert_eq!(rid, request_id, "unknown request id");
                let is_valid = external_validation(response).await?;
                sender
                    .send(is_valid)
                    .expect("`db-sync` never drops its receiver");
            }
            _ => continue,
        }
    }
}

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
        db_sync::Request::ProgramIdsAt(latest_committed_block),
        |response| async {
            let (at, program_ids) = response.unwrap_program_ids_at();
            debug_assert_eq!(at, latest_committed_block);

            let Some(program_ids) = program_ids else {
                return Ok(programs_count == 0);
            };

            if program_ids.len() as u64 != programs_count {
                return Ok(false);
            }

            let new_code_ids = router_query
                .programs_code_ids_at(program_ids, latest_committed_block)
                .await?;
            if new_code_ids.iter().any(|code_id| code_id.is_zero()) {
                return Ok(false);
            }

            Ok(true)
        },
    )
    .await?;

    let (at, program_ids) = response.unwrap_program_ids_at();
    debug_assert_eq!(at, latest_committed_block);
    let program_ids = program_ids.unwrap_or_default();

    let code_ids = router_query
        .programs_code_ids_at(program_ids.iter().copied(), latest_committed_block)
        .await?;

    debug_assert_eq!(program_ids.len(), code_ids.len());
    let program_code_ids = iter::zip(program_ids, code_ids).collect();
    Ok(program_code_ids)
}

async fn collect_code_ids(
    observer: &mut ObserverService,
    network: &mut NetworkService,
    latest_committed_block: H256,
) -> Result<BTreeSet<CodeId>> {
    let router_query = observer.router_query();
    let codes_count = router_query
        .validated_codes_count_at(latest_committed_block)
        .await?;

    let response = net_fetch(network, db_sync::Request::ValidCodes, |response| async {
        let code_ids = response.unwrap_valid_codes();

        if (code_ids.len() as u64) < codes_count {
            return Ok(false);
        }

        let code_states = router_query
            .codes_states_at(code_ids.iter().copied(), latest_committed_block)
            .await?;
        if code_states
            .into_iter()
            .any(|state| state.into() != CodeState::Validated as u8)
        {
            return Ok(false);
        }

        Ok(true)
    })
    .await?;

    let code_ids = response.unwrap_valid_codes();
    Ok(code_ids)
}

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
            let request = pending_network_requests.keys().copied().collect();
            let response = net_fetch(network, db_sync::Request::Hashes(request), |_| async {
                unreachable!()
            })
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

async fn sync_from_network(
    network: &mut NetworkService,
    db: &Database,
    code_ids: &BTreeSet<CodeId>,
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
    mut code_ids: BTreeSet<CodeId>,
) -> Result<()> {
    /// codes we instrument had already been processed by gear.exe,
    /// so generated code commitments are never going to be submitted,
    /// so we just pass placeholder value for their timestamp
    const TIMESTAMP: u64 = u64::MAX;

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
        latest_committed_block:
            BlockData {
                hash: latest_committed_block,
                header: latest_block_header,
                events: _,
            },
        previous_committed_block,
    }) = EventData::collect(observer, db, finalized_block).await?
    else {
        log::warn!("No any committed block found. Skipping fast synchronization...");
        return Ok(());
    };

    let code_ids = collect_code_ids(observer, network, latest_committed_block).await?;
    let program_code_ids =
        collect_program_code_ids(observer, network, latest_committed_block).await?;
    // we fetch program states from the finalized block
    // because actual states are at the same block as we acquired the latest committed block
    let program_states =
        collect_program_states(observer, finalized_block, &program_code_ids).await?;

    sync_from_network(network, db, &code_ids, &program_states).await;

    instrument_codes(db, compute, code_ids).await?;

    let schedule =
        ScheduleRestorer::from_storage(db, &program_states, latest_block_header.height)?.restore();

    for (program_id, code_id) in program_code_ids {
        db.set_program_code_id(program_id, code_id);
    }

    // TODO #4563: this is a temporary solution.
    // from `pre_process_genesis_for_db`
    {
        db.set_block_header(latest_committed_block, latest_block_header.clone());
        db.set_block_events(latest_committed_block, &[]);

        db.set_latest_synced_block_height(latest_block_header.height);
        db.set_block_is_synced(latest_committed_block);

        // NOTE: there is no invariant that fast sync should recover queues
        db.set_block_commitment_queue(latest_committed_block, Default::default());
        db.set_block_codes_queue(latest_committed_block, Default::default());
        db.set_previous_not_empty_block(
            latest_committed_block,
            previous_committed_block.unwrap_or_else(H256::zero),
        );
        db.set_block_program_states(latest_committed_block, program_states);
        db.set_block_schedule(latest_committed_block, schedule);
        unsafe {
            db.set_non_empty_block_outcome(latest_committed_block);
        }

        db.set_latest_computed_block(latest_committed_block, latest_block_header);

        db.set_block_computed(latest_committed_block);
    }

    log::info!("Fast synchronization done");

    #[cfg(test)]
    sender
        .send(crate::tests::utils::TestableEvent::FastSyncDone(
            latest_committed_block,
        ))
        .expect("failed to broadcast fast sync done event");

    Ok(())
}
