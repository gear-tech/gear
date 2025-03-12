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
    db::{BlockMetaStorage, CodesStorage, OnChainStorage},
    events::{BlockEvent, MirrorEvent, RouterEvent},
    gear::{CodeCommitment, StateTransition},
};
use ethexe_compute::ComputeEvent;
use ethexe_db::Database;
use ethexe_network::{db_sync, db_sync::RequestId, NetworkEvent, NetworkService};
use ethexe_observer::ObserverEvent;
use ethexe_runtime_common::{
    state::{
        ActiveProgram, DispatchStash, Mailbox, MaybeHashOf, MemoryPages, MemoryPagesRegion,
        Program, ProgramState, Waitlist,
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

#[derive(Debug, Copy, Clone, Hash, Eq, PartialEq)]
enum Request {
    ProgramState {
        program_id: ActorId,
    },
    MemoryPages,
    MemoryPagesRegions,
    Waitlist {
        program_id: ActorId,
    },
    Mailbox {
        program_id: ActorId,
    },
    DispatchStash {
        program_id: ActorId,
    },
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
    /// we convert into `pending_requests` because `RequestId` is known
    buffered_requests: HashMap<H256, HashSet<Request>>,
    /// Pending requests, we remove one by one on each hash from response
    pending_requests: HashMap<(RequestId, H256), HashSet<Request>>,
}

impl BufRequests {
    fn add(&mut self, hash: H256, request: Request) {
        self.buffered_requests
            .entry(hash)
            .or_default()
            .insert(request);
    }

    /// Returns `true` if there are pending requests
    fn request(&mut self, network: &mut NetworkService) -> bool {
        let buffered_requests = mem::take(&mut self.buffered_requests);
        if !buffered_requests.is_empty() {
            let request = buffered_requests.keys().copied().collect();
            let request_id = network.request_db_data(db_sync::Request(request));

            for (hash, requests) in buffered_requests {
                self.total_pending_requests += requests.len() as u64;
                self.pending_requests.insert((request_id, hash), requests);
            }
        }

        !self.pending_requests.is_empty()
    }

    fn complete(&mut self, request_id: RequestId, hash: H256) -> Option<HashSet<Request>> {
        let requests = self.pending_requests.remove(&(request_id, hash))?;
        self.total_completed_requests += requests.len() as u64;
        Some(requests)
    }

    /// (total completed request, total pending requests)
    fn stats(&self) -> (u64, u64) {
        debug_assert!(self.total_completed_requests <= self.total_pending_requests);
        (self.total_completed_requests, self.total_pending_requests)
    }
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

    let highest_block = observer
        .provider()
        .get_block(BlockId::Number(BlockNumberOrTag::Latest))
        .await
        .context("failed to get latest block")?
        .expect("latest block always exist");
    let highest_block = H256(highest_block.header.hash.0);

    observer.force_sync_block(highest_block).await?;
    while let Some(event) = observer.next().await {
        if let ObserverEvent::BlockSynced(synced_block) = event? {
            debug_assert_eq!(highest_block, synced_block);
            break;
        }
    }

    let (program_states, program_code_ids, latest_block, previous_block) =
        collect_event_data(db, highest_block).await?;
    let latest_block_header =
        OnChainStorage::block_header(db, latest_block).expect("observer must fulfill database");

    let mut requests = BufRequests::default();
    let mut schedule_restorer = ScheduleRestorer::new(latest_block_header.height);
    // initially fill `BufRequests` and database
    {
        for (&program_id, &state) in &program_states {
            requests.add(state, Request::ProgramState { program_id });
        }

        let mut codes_to_receive = HashSet::new();
        for (program_id, code_id) in program_code_ids {
            db.set_program_code_id(program_id, code_id);
            codes_to_receive.insert(code_id);
        }

        log::info!("Instrument {} codes", codes_to_receive.len());
        for &code_id in &codes_to_receive {
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
                codes_to_receive.remove(&id);
                if codes_to_receive.is_empty() {
                    break;
                }
            }
        }
        log::info!("Instrumentation completed");
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
                    let completed_requests = requests
                        .complete(request_id, hash)
                        .expect("unknown `db-sync` response");

                    db.write(&data);

                    for request in completed_requests {
                        match request {
                            Request::ProgramState { program_id } => {
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
                                if let Some(waitlist_hash) = waitlist_hash.hash() {
                                    requests.add(waitlist_hash, Request::Waitlist { program_id });
                                }
                                if let Some(mailbox_hash) = mailbox_hash.hash() {
                                    requests.add(mailbox_hash, Request::Mailbox { program_id });
                                }
                                if let Some(stash_hash) = stash_hash.hash() {
                                    requests.add(stash_hash, Request::DispatchStash { program_id });
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
                                let pages_region: MemoryPagesRegion =
                                    Decode::decode(&mut &data[..])
                                        .expect("`db-sync` must validate data");

                                for page_buf_hash in pages_region
                                    .as_inner()
                                    .iter()
                                    .map(|(_page, hash)| hash.hash())
                                {
                                    requests.add(page_buf_hash, Request::Data);
                                }
                            }
                            Request::Waitlist { program_id } => {
                                let waitlist: Waitlist = Decode::decode(&mut &data[..])
                                    .expect("`db-sync` must validate data");
                                schedule_restorer.waitlist(program_id, &waitlist);
                            }
                            Request::Mailbox { program_id } => {
                                let mailbox: Mailbox = Decode::decode(&mut &data[..])
                                    .expect("`db-sync` must validate data");
                                schedule_restorer.mailbox(program_id, &mailbox);
                            }
                            Request::DispatchStash { program_id } => {
                                let stash: DispatchStash = Decode::decode(&mut &data[..])
                                    .expect("`db-sync` must validate data");
                                schedule_restorer.stash(program_id, &stash);
                            }
                            Request::Data => {}
                        }
                    }
                }
            }
            Err(err) => {
                unreachable!("{err:?}");
            }
        }

        if !requests.request(network) {
            break;
        }
    }

    let (completed, pending) = requests.stats();
    log::info!("[{completed:>05} / {pending:>05}] Getting network data done");
    debug_assert_eq!(completed, pending);

    // FIXME: the block can have program states of a few future blocks
    db.set_block_program_states(latest_block, program_states);
    db.set_block_schedule(latest_block, schedule_restorer.build());
    db.set_block_commitment_queue(latest_block, VecDeque::new());
    // `latest_block` is committed and thus cannot be empty,
    // so we just insert placeholder value to pass emptiness check
    db.set_block_outcome(latest_block, vec![StateTransition::default()]);
    // and `previous_block` is committed too
    db.set_previous_not_empty_block(latest_block, previous_block.unwrap_or_else(H256::zero));
    db.set_block_computed(latest_block);
    db.set_latest_computed_block(latest_block, latest_block_header);

    log::info!("Fast synchronization done");

    Ok(())
}

async fn collect_event_data(
    db: &Database,
    highest_block: H256,
) -> Result<(
    BTreeMap<ActorId, H256>,
    Vec<(ActorId, CodeId)>,
    H256,
    Option<H256>,
)> {
    let mut states = BTreeMap::new();
    let mut program_code_ids = Vec::new();
    let mut previous_block = None;
    let mut latest_block = None;

    let mut block = highest_block;
    while !db.block_computed(block) {
        let events = db
            .block_events(block)
            .ok_or_else(|| anyhow!("no events found for block {block}"))?;

        // we only care about the latest events
        for event in events.into_iter().rev() {
            match event {
                BlockEvent::Mirror {
                    actor_id,
                    event: MirrorEvent::StateChanged { state_hash },
                } => {
                    states.entry(actor_id).or_insert(state_hash);
                }
                BlockEvent::Router(RouterEvent::BlockCommitted { hash }) => {
                    if latest_block.is_some() {
                        previous_block.get_or_insert(hash);
                    } else {
                        latest_block = Some(hash);
                    }
                }
                BlockEvent::Router(RouterEvent::ProgramCreated { actor_id, code_id }) => {
                    program_code_ids.push((actor_id, code_id));
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
        let latest_block_header =
            OnChainStorage::block_header(db, latest_block).expect("observer must fulfill database");
        let previous_block_header = OnChainStorage::block_header(db, previous_block)
            .expect("observer must fulfill database");
        assert!(previous_block_header.height < latest_block_header.height);
    }

    Ok((states, program_code_ids, latest_block, previous_block))
}
