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
    collections::{BTreeMap, HashMap, HashSet, VecDeque},
    mem,
};

struct EventData {
    program_states: BTreeMap<ActorId, H256>,
    program_code_ids: Vec<(ActorId, CodeId)>,
    code_ids: HashSet<CodeId>,
    latest_block: H256,
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

            // we only care about the latest events
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

        let latest_block = latest_block.context("no blocks committed")?;

        #[cfg(debug_assertions)]
        if let Some(previous_block) = previous_block {
            let latest_block_header = OnChainStorage::block_header(db, latest_block)
                .expect("observer must fulfill database");
            let previous_block_header = OnChainStorage::block_header(db, previous_block)
                .expect("observer must fulfill database");
            assert!(previous_block_header.height < latest_block_header.height);
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

#[derive(Debug, Copy, Clone, Hash, Eq, PartialEq)]
enum Request {
    ProgramState,
    MemoryPages,
    MemoryPagesRegions,
    Waitlist,
    Mailbox,
    UserMailbox,
    DispatchStash,
    /// We don't care about an actual type of the request
    /// because we will just insert data into the database
    Data,
}

#[derive(Debug, Default)]
struct BufRequests {
    /// Total completed requests
    total_completed_requests: u64,
    /// Total pending requests
    total_pending_requests: u64,

    /// Buffered requests we convert into one network request, and after that
    /// we convert them into `pending_requests` because `RequestId` is known
    buffered_requests: HashMap<H256, Request>,
    /// Pending requests, we remove one by one on each hash from a response
    pending_requests: HashMap<(RequestId, H256), Request>,
}

impl BufRequests {
    fn add(&mut self, hash: H256, request: Request) {
        self.buffered_requests.insert(hash, request);
    }

    /// Returns `true` if there are pending requests
    fn request(&mut self, network: &mut NetworkService) -> bool {
        let buffered_requests = mem::take(&mut self.buffered_requests);
        if !buffered_requests.is_empty() {
            let request = buffered_requests.keys().copied().collect();
            let request_id = network.db_sync().request(db_sync::Request(request));

            for (hash, request) in buffered_requests {
                self.total_pending_requests += 1;
                self.pending_requests.insert((request_id, hash), request);
            }
        }

        !self.pending_requests.is_empty()
    }

    fn complete(&mut self, request_id: RequestId, hash: H256) -> Option<Request> {
        let requests = self.pending_requests.remove(&(request_id, hash))?;
        self.total_completed_requests += 1;
        Some(requests)
    }

    /// (total completed request, total pending requests)
    fn stats(&self) -> (u64, u64) {
        debug_assert!(self.total_completed_requests <= self.total_pending_requests);
        (self.total_completed_requests, self.total_pending_requests)
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
    let mut requests = BufRequests::default();
    let mut states = HashMap::<H256, HashSet<ActorId>>::new();
    let mut mailbox_states = HashMap::new();
    for (&program_id, &state) in program_states {
        requests.add(state, Request::ProgramState);
        states.entry(state).or_default().insert(program_id);
    }
    requests.request(network);

    while let Some(event) = network.next().await {
        let NetworkEvent::DbResponse { request_id, result } = event else {
            continue;
        };

        let (completed, pending) = requests.stats();
        log::info!("[{completed:>05} / {pending:>05}] Getting network data");

        match result {
            Ok(db_sync::Response(data)) => {
                for (hash, data) in data {
                    let request = requests
                        .complete(request_id, hash)
                        .expect("unknown `db-sync` response");

                    db.write(&data);

                    match request {
                        Request::ProgramState => {
                            let state: ProgramState = Decode::decode(&mut &data[..])
                                .expect("`db-sync` must validate data");

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
                                    requests.add(allocations_hash, Request::Data);
                                }
                                if let Some(pages_hash) = pages_hash.hash() {
                                    requests.add(pages_hash, Request::MemoryPages);
                                }
                            }

                            if let Some(queue_hash) = queue_hash.hash() {
                                requests.add(queue_hash, Request::Data);
                            }

                            let program_ids = states.remove(&hash).expect("unknown program state");

                            if let Some(waitlist_hash) = waitlist_hash.hash() {
                                requests.add(waitlist_hash, Request::Waitlist);
                                states
                                    .entry(waitlist_hash)
                                    .insert_entry(program_ids.clone());
                            }
                            if let Some(mailbox_hash) = mailbox_hash.hash() {
                                requests.add(mailbox_hash, Request::Mailbox);
                                states.entry(mailbox_hash).insert_entry(program_ids.clone());
                            }
                            if let Some(stash_hash) = stash_hash.hash() {
                                requests.add(stash_hash, Request::DispatchStash);
                                states.entry(stash_hash).insert_entry(program_ids.clone());
                            }
                        }
                        Request::MemoryPages => {
                            let memory_pages: MemoryPages = Decode::decode(&mut &data[..])
                                .expect("`db-sync` must validate data");

                            for pages_region_hash in memory_pages
                                .to_inner()
                                .into_iter()
                                .flat_map(MaybeHashOf::hash)
                            {
                                requests.add(pages_region_hash, Request::MemoryPagesRegions);
                            }
                        }
                        Request::MemoryPagesRegions => {
                            let pages_region: MemoryPagesRegion = Decode::decode(&mut &data[..])
                                .expect("`db-sync` must validate data");

                            for page_buf_hash in pages_region
                                .as_inner()
                                .iter()
                                .map(|(_page, hash)| hash.hash())
                            {
                                requests.add(page_buf_hash, Request::Data);
                            }
                        }
                        Request::Waitlist => {
                            let waitlist: Waitlist = Decode::decode(&mut &data[..])
                                .expect("`db-sync` must validate data");
                            let program_ids = states.remove(&hash).expect("unknown waitlist");
                            for program_id in program_ids {
                                schedule_restorer.waitlist(program_id, &waitlist);
                            }
                        }
                        Request::Mailbox => {
                            let mailbox: Mailbox = Decode::decode(&mut &data[..])
                                .expect("`db-sync` must validate data");
                            let program_ids = states.remove(&hash).expect("unknown mailbox");
                            for (&user_id, user_mailbox) in mailbox.as_ref() {
                                requests.add(user_mailbox.hash(), Request::UserMailbox);
                                mailbox_states
                                    .insert(user_mailbox.hash(), (program_ids.clone(), user_id));
                            }
                        }
                        Request::UserMailbox => {
                            let user_mailbox: UserMailbox = Decode::decode(&mut &data[..])
                                .expect("`db-sync` must validate data");
                            let (program_ids, user_id) =
                                mailbox_states.remove(&hash).expect("unknown user mailbox");
                            for program_id in program_ids {
                                schedule_restorer.mailbox(program_id, user_id, &user_mailbox);
                            }
                        }
                        Request::DispatchStash => {
                            let stash: DispatchStash = Decode::decode(&mut &data[..])
                                .expect("`db-sync` must validate data");
                            let program_ids = states.remove(&hash).expect("unknown waitlist state");
                            for program_id in program_ids {
                                schedule_restorer.stash(program_id, &stash);
                            }
                        }
                        Request::Data => {}
                    }
                }
            }
            Err((request, err)) => {
                network.db_sync().retry(request);
                log::warn!("{request_id:?} failed: {err}. Retrying...");
            }
        }

        if !requests.request(network) {
            break;
        }
    }

    let (completed, pending) = requests.stats();
    log::info!("[{completed:>05} / {pending:>05}] Getting network data done");
    debug_assert_eq!(completed, pending);

    schedule_restorer.restore()
}

async fn instrument_codes(
    db: &Database,
    compute: &mut ComputeService,
    mut code_ids: HashSet<CodeId>,
) -> Result<()> {
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
        ..
    } = service;
    let Some(network) = network else {
        log::warn!("Fast synchronization has been skipped because network service is disabled");
        return Ok(());
    };

    log::info!("Fast synchronization is in progress...");

    let finalized_block = sync_finalized_head(observer).await?;
    let event_data = EventData::collect(db, finalized_block).await?;

    instrument_codes(db, compute, event_data.code_ids).await?;

    let latest_block = event_data.latest_block;
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

    Ok(())
}
