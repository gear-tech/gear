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

use crate::{Event, Service};
use alloy::{
    eips::{BlockId, BlockNumberOrTag},
    providers::Provider,
};
use anyhow::{anyhow, Context, Result};
use ethexe_common::{
    db::{BlockHeader, BlockMetaStorage, CodesStorage, OnChainStorage, Schedule},
    events::{BlockEvent, MirrorEvent, RouterEvent},
    gear::CodeCommitment,
};
use ethexe_compute::{ComputeEvent, ComputeService};
use ethexe_db::Database;
use ethexe_network::{db_sync, db_sync::RequestId, NetworkEvent, NetworkService};
use ethexe_observer::{ObserverEvent, ObserverService};
use ethexe_runtime_common::{
    state::{
        ActiveProgram, DispatchStash, Mailbox, MaybeHashOf, MemoryPages, MemoryPagesRegion,
        Program, ProgramState, UserMailbox, Waitlist,
    },
    ScheduleRestorer,
};
use futures::StreamExt;
use gprimitives::{ActorId, CodeId, H256};
use parity_scale_codec::Decode;
use std::{
    collections::{BTreeMap, BTreeSet, HashMap, HashSet, VecDeque},
    pin::pin,
};

struct EventData {
    program_states: BTreeMap<ActorId, H256>,
    program_code_ids: Vec<(ActorId, CodeId)>,
    code_ids: HashSet<CodeId>,
    latest_block: Option<H256>,
    previous_block: Option<H256>,
}

impl EventData {
    async fn collect(db: &Database, highest_block: H256) -> Result<Self> {
        let mut program_states = BTreeMap::new();
        let mut program_code_ids = Vec::new();
        let mut code_ids = HashSet::new();
        let mut previous_block = None;
        let mut latest_block = None;

        let mut block = highest_block;
        while !db.block_computed(block) {
            let events = db
                .block_events(block)
                .ok_or_else(|| anyhow!("no events found for block {block}"))?;

            // NOTE: logic relies on events in order as they are emitted on Ethereum
            for event in events.into_iter().rev() {
                match event {
                    BlockEvent::Mirror {
                        actor_id,
                        event: MirrorEvent::StateChanged { state_hash },
                    } if latest_block.is_some() => {
                        program_states.entry(actor_id).or_insert(state_hash);
                    }
                    BlockEvent::Router(RouterEvent::BlockCommitted { hash }) => {
                        if latest_block.is_some() {
                            previous_block.get_or_insert(hash);
                        } else {
                            latest_block = Some(hash);
                        }
                    }
                    BlockEvent::Router(RouterEvent::ProgramCreated { actor_id, code_id })
                        if latest_block.is_some() =>
                    {
                        program_code_ids.push((actor_id, code_id));
                    }
                    BlockEvent::Router(RouterEvent::CodeGotValidated {
                        code_id,
                        valid: true,
                    }) => {
                        if !db.instrumented_code_exists(ethexe_runtime::VERSION, code_id) {
                            code_ids.insert(code_id);
                        }
                    }
                    _ => {}
                }
            }

            let header = OnChainStorage::block_header(db, block)
                .ok_or_else(|| anyhow!("header not found for synced block {block}"))?;
            let parent = header.parent_hash;
            block = parent;
        }

        if let Some(latest_block) = latest_block {
            // recover data we haven't seen in events by the latest computed block
            // TODO: .context()
            if let Some((computed_block, _computed_header)) = db.latest_computed_block() {
                let computed_program_states = db
                    .block_program_states(computed_block)
                    .context("program states of latest computed block not found")?;
                for (program_id, state) in computed_program_states {
                    program_states.entry(program_id).or_insert(state);
                }
            }

            #[cfg(debug_assertions)]
            if let Some(previous_block) = previous_block {
                let latest_block_header = OnChainStorage::block_header(db, latest_block)
                    .expect("observer must fulfill database");
                let previous_block_header = OnChainStorage::block_header(db, previous_block)
                    .expect("observer must fulfill database");
                assert!(previous_block_header.height < latest_block_header.height);
            }
        }

        Ok(Self {
            program_states,
            program_code_ids,
            code_ids,
            latest_block,
            previous_block,
        })
    }
}

#[derive(Debug, Clone, Copy, Eq, PartialEq)]
enum RequestMetadata<'a> {
    ProgramState {
        program_ids: &'a HashSet<ActorId>,
    },
    MemoryPages,
    MemoryPagesRegion,
    Waitlist {
        program_ids: &'a HashSet<ActorId>,
    },
    Mailbox {
        program_ids: &'a HashSet<ActorId>,
    },
    UserMailbox {
        program_ids: &'a HashSet<ActorId>,
        user_id: ActorId,
    },
    DispatchStash {
        program_ids: &'a HashSet<ActorId>,
    },
    /// Any data we only insert into the database.
    Data,
}

impl RequestMetadata<'_> {
    /// Simple request metadata means that there is no need to process its content
    /// if it's already in the database because it has no fields that affect data flow.
    ///
    /// For example, `ProgramState` has a list of program IDs,
    /// so we need to process the whole request tree up to `Waitlist`, `UserMailbox` and others
    /// because they can change `ScheduleRestorer`.
    fn is_simple(self) -> bool {
        matches!(self, RequestMetadata::Data)
    }
}

#[derive(Debug, Default)]
struct RequestManager<'a> {
    /// Total completed requests
    total_completed_requests: u64,
    /// Total pending requests
    total_pending_requests: u64,

    /// Buffered requests are either:
    /// * Skipped if they are simple
    /// * Completed if the database has keys
    /// * Converted into one network request, and after that
    ///   we convert them into `pending_requests` because `RequestId` is known
    buffered_requests: HashMap<H256, RequestMetadata<'a>>,
    /// Pending requests, we remove one by one on each hash from a network response
    pending_requests: HashMap<(RequestId, H256), RequestMetadata<'a>>,
    /// Completed requests
    responses: HashMap<H256, (RequestMetadata<'a>, Vec<u8>)>,
}

impl<'a> RequestManager<'a> {
    fn add(&mut self, hash: H256, metadata: RequestMetadata<'a>) {
        let old = self.buffered_requests.insert(hash, metadata);

        #[cfg(debug_assertions)]
        if let Some(old_request) = old {
            assert_eq!(metadata, old_request);
        }
    }

    fn request(&mut self, network: &mut NetworkService, db: &Database) -> bool {
        debug_assert!(!self.buffered_requests.is_empty());

        let mut network_request = BTreeSet::new();
        let mut pending_requests = HashMap::new();
        for (hash, metadata) in self.buffered_requests.drain() {
            if metadata.is_simple() && db.has_hash(hash) {
                continue;
            }

            if let Some(data) = db.read_by_hash(hash) {
                self.responses.insert(hash, (metadata, data));
                self.total_completed_requests += 1;
            } else {
                network_request.insert(hash);
                pending_requests.insert(hash, metadata);
            }

            self.total_pending_requests += 1;
        }

        if !network_request.is_empty() {
            let request_id = network.db_sync().request(db_sync::Request(network_request));
            for (hash, metadata) in pending_requests {
                self.pending_requests.insert((request_id, hash), metadata);
            }
        }

        !self.pending_requests.is_empty()
    }

    fn handle_response(
        &mut self,
        request_id: RequestId,
        response: db_sync::Response,
        db: &Database,
    ) {
        let db_sync::Response(data) = response;

        for (hash, data) in data {
            let metadata = self
                .pending_requests
                .remove(&(request_id, hash))
                .expect("unknown pending request");

            let db_hash = db.write(&data);
            debug_assert_eq!(hash, db_hash);

            self.responses.insert(hash, (metadata, data));
            self.total_completed_requests += 1;
        }
    }

    fn take_responses(&mut self) -> Vec<(RequestMetadata<'a>, Vec<u8>)> {
        self.responses.drain().map(|(_hash, value)| value).collect()
    }

    fn any_new_requests(&self) -> bool {
        self.buffered_requests.is_empty()
    }

    /// (total completed request, total pending requests)
    fn stats(&self) -> (u64, u64) {
        let completed = self.total_completed_requests;
        let pending = self.total_pending_requests;
        debug_assert!(completed <= pending, "{completed} <= {pending}");
        (completed, pending)
    }
}

impl Drop for RequestManager<'_> {
    fn drop(&mut self) {
        #[cfg(debug_assertions)]
        {
            let Self {
                total_completed_requests,
                total_pending_requests,
                buffered_requests,
                pending_requests,
                responses,
            } = self;
            assert_eq!(total_completed_requests, total_pending_requests);
            assert_eq!(*buffered_requests, HashMap::new());
            assert_eq!(*pending_requests, HashMap::new());
            assert_eq!(*responses, HashMap::new());
        }
    }
}

async fn sync_finalized_head(observer: &mut ObserverService) -> Result<H256> {
    let highest_block = observer
        .provider()
        // we get finalized block to avoid block reorganization
        // because we restore the database only for the latest block of a chain,
        // and thus the reorganization can lead us to an empty block
        .get_block(BlockId::Number(BlockNumberOrTag::Finalized))
        .await
        .context("failed to get latest block")?
        .expect("latest block always exist");
    let highest_block = H256(highest_block.header.hash.0);

    log::info!("Syncing chain head {highest_block}");
    observer.force_sync_block(highest_block).await?;
    while let Some(event) = observer.next().await {
        match event? {
            ObserverEvent::Blob(_blob) => {
                unreachable!("no blob events should occur before chain head is synced")
            }
            ObserverEvent::Block(_) => {}
            ObserverEvent::BlockSynced(data) => {
                debug_assert_eq!(highest_block, data.block_hash);
                break;
            }
        }
    }

    Ok(highest_block)
}

async fn sync_from_network(
    network: &mut NetworkService,
    db: &Database,
    program_states: &BTreeMap<ActorId, H256>,
    latest_block_header: &BlockHeader,
) -> Schedule {
    let mut schedule_restorer = ScheduleRestorer::new(latest_block_header.height);

    let program_states: HashMap<H256, HashSet<ActorId>> =
        program_states
            .iter()
            .fold(HashMap::new(), |mut acc, (&program_id, &state)| {
                acc.entry(state).or_default().insert(program_id);
                acc
            });
    let mut manager = RequestManager::default();
    for (&state, program_ids) in &program_states {
        manager.add(state, RequestMetadata::ProgramState { program_ids });
    }

    loop {
        let wait_for_network = manager.request(network, db);

        let (completed, pending) = manager.stats();
        log::info!("[{completed:>05} / {pending:>05}] Getting network data");

        if wait_for_network {
            let stream = network.filter_map(|event| async move {
                if let NetworkEvent::DbResponse { request_id, result } = event {
                    Some((request_id, result))
                } else {
                    None
                }
            });
            let mut stream = pin!(stream);
            let (request_id, result) = stream
                .next()
                .await
                .expect("network service stream is infinite");

            match result {
                Ok(response) => {
                    manager.handle_response(request_id, response, db);
                }
                Err((request, err)) => {
                    network.db_sync().retry(request);
                    log::warn!("{request_id:?} failed: {err}. Retrying...");
                    continue;
                }
            }
        }

        for (metadata, data) in manager.take_responses() {
            match metadata {
                RequestMetadata::ProgramState { program_ids } => {
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
                        manager.add(queue_hash, RequestMetadata::Data);
                    }

                    if let Some(waitlist_hash) = waitlist_hash.hash() {
                        manager.add(waitlist_hash, RequestMetadata::Waitlist { program_ids });
                    }
                    if let Some(mailbox_hash) = mailbox_hash.hash() {
                        manager.add(mailbox_hash, RequestMetadata::Mailbox { program_ids });
                    }
                    if let Some(stash_hash) = stash_hash.hash() {
                        manager.add(stash_hash, RequestMetadata::DispatchStash { program_ids });
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

                    for page_buf_hash in pages_region
                        .as_inner()
                        .iter()
                        .map(|(_page, hash)| hash.hash())
                    {
                        manager.add(page_buf_hash, RequestMetadata::Data);
                    }
                }
                RequestMetadata::Waitlist { program_ids } => {
                    let waitlist: Waitlist =
                        Decode::decode(&mut &data[..]).expect("`db-sync` must validate data");
                    for &program_id in program_ids {
                        schedule_restorer.waitlist(program_id, &waitlist);
                    }
                }
                RequestMetadata::Mailbox { program_ids } => {
                    let mailbox: Mailbox =
                        Decode::decode(&mut &data[..]).expect("`db-sync` must validate data");
                    for (&user_id, user_mailbox) in mailbox.as_ref() {
                        manager.add(
                            user_mailbox.hash(),
                            RequestMetadata::UserMailbox {
                                program_ids,
                                user_id,
                            },
                        );
                    }
                }
                RequestMetadata::UserMailbox {
                    program_ids,
                    user_id,
                } => {
                    let user_mailbox: UserMailbox =
                        Decode::decode(&mut &data[..]).expect("`db-sync` must validate data");
                    for &program_id in program_ids {
                        schedule_restorer.mailbox(program_id, user_id, &user_mailbox);
                    }
                }
                RequestMetadata::DispatchStash { program_ids } => {
                    let stash: DispatchStash =
                        Decode::decode(&mut &data[..]).expect("`db-sync` must validate data");
                    for &program_id in program_ids {
                        schedule_restorer.stash(program_id, &stash);
                    }
                }
                RequestMetadata::Data => {}
            }
        }

        if manager.any_new_requests() {
            break;
        }
    }

    let (completed, pending) = manager.stats();
    log::info!("[{completed:>05} / {pending:>05}] Getting network data done");

    schedule_restorer.restore()
}

async fn instrument_codes(
    db: &Database,
    compute: &mut ComputeService,
    mut code_ids: HashSet<CodeId>,
) -> Result<()> {
    if code_ids.is_empty() {
        log::info!("No codes to instrument. Skipping...");
        return Ok(());
    }

    log::info!("Instrument {} codes", code_ids.len());

    for &code_id in &code_ids {
        let code_info = db
            .code_blob_info(code_id)
            .expect("observer must fulfill database");
        let original_code = db
            .original_code(code_id)
            .expect("observer must fulfill database");
        compute.receive_code(code_id, code_info.timestamp, original_code);
    }

    while let Some(event) = compute.next().await {
        if let ComputeEvent::CodeProcessed(CodeCommitment { id, .. }) = event? {
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
        sender,
        ..
    } = service;
    let Some(network) = network else {
        log::warn!("Network service is disabled. Skipping...");
        return Ok(());
    };

    log::info!("Fast synchronization is in progress...");

    let finalized_block = sync_finalized_head(observer).await?;
    let event_data = EventData::collect(db, finalized_block).await?;

    instrument_codes(db, compute, event_data.code_ids).await?;

    let Some(latest_block) = event_data.latest_block else {
        log::info!("No any committed block found. Skipping...");
        return Ok(());
    };
    let latest_block_header =
        OnChainStorage::block_header(db, latest_block).expect("observer must fulfill database");

    let schedule = sync_from_network(
        network,
        db,
        &event_data.program_states,
        &latest_block_header,
    )
    .await;

    for (program_id, code_id) in event_data.program_code_ids {
        db.set_program_code_id(program_id, code_id);
    }

    // NOTE: there is no invariant that fast sync should recover queues
    db.set_block_commitment_queue(latest_block, VecDeque::new());
    db.set_block_codes_queue(latest_block, VecDeque::new());

    db.set_block_program_states(latest_block, event_data.program_states);
    db.set_block_schedule(latest_block, schedule);
    unsafe {
        db.set_non_empty_block_outcome(latest_block);
    }
    db.set_previous_not_empty_block(
        latest_block,
        event_data.previous_block.unwrap_or_else(H256::zero),
    );
    db.set_block_computed(latest_block);
    db.set_latest_computed_block(latest_block, latest_block_header);

    log::info!("Fast synchronization done");

    // Broadcast service started event.
    // Never supposed to be Some in production code.
    if let Some(sender) = sender.as_ref() {
        sender
            .send(Event::FastSyncDone(latest_block))
            .expect("failed to broadcast service STARTED event");
    }

    Ok(())
}
