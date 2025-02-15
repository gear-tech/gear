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
use anyhow::Result;
use ethexe_common::{
    db::{BlockMetaStorage, CodesStorage},
    events::{BlockEvent, MirrorEvent, RouterEvent},
};
use ethexe_network::{db_sync, db_sync::RequestId, NetworkEvent, NetworkService};
use ethexe_observer::Query;
use ethexe_runtime_common::{
    state::{
        ActiveProgram, DispatchStash, Mailbox, MaybeHashOf, MemoryPages, MemoryPagesRegion,
        Program, ProgramState, Storage, Waitlist,
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
            let request_id = network.request_db_data(db_sync::Request::DataForHashes(request));

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
        network,
        router_query,
        query,
        db,
        ..
    } = service;
    let Some(network) = network else {
        log::warn!("Fast synchronization has been skipped because network service is disabled");
        return Ok(());
    };

    log::info!("Fast synchronization is in progress...");

    let latest_block = router_query.latest_committed_block_hash().await?;
    debug_assert_ne!(latest_block, H256::zero(), "router is not deployed");
    let latest_block_header = query.get_block_header_meta(latest_block).await?;

    let chain = query.get_last_committed_chain(latest_block).await?;
    let (program_states, program_code_ids) = collect_event_data(query, &chain).await?;
    log::info!("Processing {} blocks", chain.len());

    let mut requests = BufRequests::default();
    // initially fill `BufRequests` and database
    {
        for (&program_id, &state) in &program_states {
            requests.add(state, Request::ProgramState { program_id });
        }

        db.set_block_end_program_states(latest_block, program_states);

        for (program_id, code_id) in program_code_ids {
            db.set_program_code_id(program_id, code_id);
            requests.add(code_id.into_bytes().into(), Request::Data);
        }
    }
    requests.request(network);

    let mut schedule_builder = ScheduleRestorer::new(latest_block_header.height);

    while let Some(event) = network.next().await {
        let (completed, pending) = requests.stats();
        log::error!("[{completed:>05} / {pending:>05}] Processing synchronization");

        let NetworkEvent::DbResponse { request_id, result } = event else {
            continue;
        };

        match result {
            Ok(db_sync::Response::DataForHashes(data)) => {
                for (hash, data) in data {
                    let completed_requests = requests
                        .complete(request_id, hash)
                        .expect("unknown `db-sync` response");

                    for request in completed_requests {
                        match request {
                            Request::ProgramState { program_id } => {
                                let state: ProgramState = Decode::decode(&mut &data[..])
                                    .expect("`db-sync` must validate data");

                                log::error!("Program state {state:?}");

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

                                db.write_state(state);
                            }
                            Request::MemoryPages => {
                                let memory_pages: MemoryPages = Decode::decode(&mut &data[..])
                                    .expect("`db-sync` must validate data");

                                log::error!("Memory pages: {memory_pages:?}");

                                for pages_region_hash in memory_pages
                                    .to_inner()
                                    .into_iter()
                                    .flat_map(MaybeHashOf::hash)
                                {
                                    requests.add(pages_region_hash, Request::MemoryPagesRegions);
                                }

                                db.write_pages(memory_pages);
                            }
                            Request::MemoryPagesRegions => {
                                let pages_region: MemoryPagesRegion =
                                    Decode::decode(&mut &data[..])
                                        .expect("`db-sync` must validate data");

                                log::error!("Memory pages region: {pages_region:?}");

                                for page_buf_hash in pages_region
                                    .as_inner()
                                    .iter()
                                    .map(|(_page, hash)| hash.hash())
                                {
                                    requests.add(page_buf_hash, Request::Data);
                                }

                                db.write_pages_region(pages_region);
                            }
                            Request::Waitlist { program_id } => {
                                let waitlist: Waitlist = Decode::decode(&mut &data[..])
                                    .expect("`db-sync` must validate data");
                                schedule_builder.waitlist(program_id, &waitlist);
                                db.write_waitlist(waitlist);
                            }
                            Request::Mailbox { program_id } => {
                                let mailbox: Mailbox = Decode::decode(&mut &data[..])
                                    .expect("`db-sync` must validate data");
                                schedule_builder.mailbox(program_id, &mailbox);
                                db.write_mailbox(mailbox);
                            }
                            Request::DispatchStash { program_id } => {
                                let stash: DispatchStash = Decode::decode(&mut &data[..])
                                    .expect("`db-sync` must validate data");
                                schedule_builder.stash(program_id, &stash);
                                db.write_stash(stash);
                            }
                            Request::Data => {
                                log::error!("New data: {} bytes", data.len());
                                db.write(&data);
                            }
                        }
                    }
                }
            }
            Ok(db_sync::Response::ProgramIds(_ids)) => {
                unreachable!();
            }
            Err(err) => {
                unreachable!("{err:?}");
            }
        }

        if !requests.request(network) {
            break;
        }
    }

    db.set_block_end_state_is_valid(latest_block, true);
    db.set_latest_valid_block(latest_block, latest_block_header);
    db.set_block_commitment_queue(latest_block, VecDeque::new());
    db.set_block_end_schedule(latest_block, schedule_builder.build());
    db.set_block_is_empty(latest_block, true);
    db.set_previous_committed_block(latest_block, H256::zero());

    let (completed, pending) = requests.stats();
    log::info!("[{completed:>05} / {pending:>05}] Fast synchronization done");
    debug_assert_eq!(completed, pending);

    Ok(())
}

async fn collect_event_data(
    query: &mut Query,
    blocks: &[H256],
) -> Result<(BTreeMap<ActorId, H256>, Vec<(ActorId, CodeId)>)> {
    let mut states = BTreeMap::new();
    let mut program_code_ids = Vec::new();

    for &block in blocks.iter().rev() {
        let events = query.get_block_events(block).await?;

        for event in events {
            match event {
                BlockEvent::Mirror {
                    actor_id,
                    event: MirrorEvent::StateChanged { state_hash },
                } => {
                    states.insert(actor_id, state_hash);
                }
                BlockEvent::Router(RouterEvent::ProgramCreated { actor_id, code_id }) => {
                    program_code_ids.push((actor_id, code_id));
                }
                _ => {}
            }
        }
    }

    Ok((states, program_code_ids))
}
