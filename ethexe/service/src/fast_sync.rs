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
    providers::{Provider, RootProvider},
    transports::BoxTransport,
};
use anyhow::{Context, Result};
use ethexe_common::{
    db::{BlockMetaStorage, CodesStorage},
    events::{BlockRequestEvent, RouterRequestEvent},
};
use ethexe_db::Database;
use ethexe_ethereum::{
    mirror::MirrorQuery,
    primitives::{Address, U256},
    router::RouterQuery,
};
use ethexe_network::{db_sync, db_sync::RequestId, NetworkEvent, NetworkService};
use ethexe_runtime_common::state::{
    ActiveProgram, HashOf, MaybeHashOf, MemoryPages, MemoryPagesRegion, Program, ProgramState,
    Storage,
};
use futures::StreamExt;
use gprimitives::{ActorId, CodeId, H256};
use parity_scale_codec::Decode;
use std::{
    collections::{HashMap, HashSet},
    mem,
};

#[derive(Debug, Copy, Clone, Hash, Eq, PartialEq)]
enum RequestKind {
    ProgramState,
    MemoryPages,
    MemoryPagesRegions,
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
    buffered_requests: HashSet<(RequestKind, H256)>,
    /// Pending requests, we remove one by one on each hash from response
    pending_requests: HashMap<(RequestId, H256), RequestKind>,
}

impl BufRequests {
    fn add(&mut self, request: RequestKind, hashes: impl IntoIterator<Item = H256>) {
        for hash in hashes {
            self.buffered_requests.insert((request, hash));
        }
    }

    /// Returns `true` if there are pending requests
    fn request(&mut self, network: &mut NetworkService) -> bool {
        let buffered_requests = mem::take(&mut self.buffered_requests);
        if !buffered_requests.is_empty() {
            let request = buffered_requests
                .iter()
                .map(|(_kind, hash)| hash)
                .copied()
                .collect();
            let request_id = network.request_db_data(db_sync::Request::DataForHashes(request));

            self.total_pending_requests += buffered_requests.len() as u64;

            for (request, hash) in buffered_requests {
                self.pending_requests.insert((request_id, hash), request);
            }
        }

        !self.pending_requests.is_empty()
    }

    fn complete(&mut self, request_id: RequestId, hash: H256) -> Option<RequestKind> {
        let kind = self.pending_requests.remove(&(request_id, hash))?;
        self.total_completed_requests += 1;
        Some(kind)
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
    let chain = query.get_last_committed_chain(latest_block).await?;

    let programs_states =
        collect_programs_states(&chain, db, observer.provider().root().clone()).await?;

    log::info!("Processing {} blocks", programs_states.len());
    let mut requests = BufRequests::default();

    for (block_hash, states) in programs_states {
        log::error!("STATES: {states:?}");

        // we assume the network returns everything we requested
        // or waits for a chance to get the rest of data
        db.set_block_end_state_is_valid(block_hash, true);

        if states.is_empty() {
            continue;
        }

        for (program_id, code_id, state) in states {
            db.set_program_code_id(program_id, code_id);
            requests.add(RequestKind::Data, Some(code_id.into_bytes().into()));
            requests.add(RequestKind::ProgramState, Some(state));
        }
    }
    requests.request(network);

    while let Some(event) = network.next().await {
        let (completed, pending) = requests.stats();
        log::error!("[{completed:>05} / {pending:>05}] Processing synchronization");

        match event {
            NetworkEvent::DbResponse { request_id, result } => match result {
                Ok(db_sync::Response::DataForHashes(data)) => {
                    for (hash, data) in data {
                        let request = requests
                            .complete(request_id, hash)
                            .expect("unknown `db-sync` response");

                        match request {
                            RequestKind::ProgramState => {
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
                                    requests.add(
                                        RequestKind::Data,
                                        allocations_hash.hash().map(HashOf::hash),
                                    );

                                    requests.add(
                                        RequestKind::MemoryPages,
                                        pages_hash.hash().map(HashOf::hash),
                                    );
                                }

                                requests.add(
                                    RequestKind::Data,
                                    [
                                        queue_hash.hash().map(HashOf::hash),
                                        waitlist_hash.hash().map(HashOf::hash),
                                        stash_hash.hash().map(HashOf::hash),
                                        mailbox_hash.hash().map(HashOf::hash),
                                    ]
                                    .into_iter()
                                    .flatten(),
                                );

                                db.write_state(state);
                            }
                            RequestKind::MemoryPages => {
                                let memory_pages: MemoryPages = Decode::decode(&mut &data[..])
                                    .expect("`db-sync` must validate data");

                                log::error!("Memory pages: {memory_pages:?}");

                                requests.add(
                                    RequestKind::MemoryPagesRegions,
                                    memory_pages
                                        .to_inner()
                                        .map(MaybeHashOf::hash)
                                        .into_iter()
                                        .flatten()
                                        .map(HashOf::hash),
                                );

                                db.write_pages(memory_pages);
                            }
                            RequestKind::MemoryPagesRegions => {
                                let pages_region: MemoryPagesRegion =
                                    Decode::decode(&mut &data[..])
                                        .expect("`db-sync` must validate data");

                                log::error!("Memory pages region: {pages_region:?}");

                                requests.add(
                                    RequestKind::Data,
                                    pages_region
                                        .as_inner()
                                        .iter()
                                        .map(|(_page, hash)| hash.hash()),
                                );

                                db.write_pages_region(pages_region);
                            }
                            RequestKind::Data => {
                                log::error!("New data: {} bytes", data.len());
                                db.write(&data);
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
            },
            NetworkEvent::ExternalValidation(response) => {
                let res = process_response_validation(&response, router_query).await?;
                let res = if res { Ok(response) } else { Err(response) };
                network.request_validated(res);
            }
            _ => {}
        }

        if !requests.request(network) {
            break;
        }
    }

    let (completed, pending) = requests.stats();
    log::info!("[{completed:>05} / {pending:>05}] Fast synchronization done");

    Ok(())
}

async fn collect_programs_states(
    blocks: &[H256],
    db: &Database,
    provider: RootProvider<BoxTransport>,
) -> Result<Vec<(H256, Vec<(ActorId, CodeId, H256)>)>> {
    let mut handled_blocks = Vec::new();
    for &block in blocks {
        let events = db
            .block_events(block)
            .context("`get_last_committed_chain` must insert block events")?;

        let mut states = Vec::new();
        for event in events {
            if let BlockRequestEvent::Router(RouterRequestEvent::ProgramCreated {
                actor_id,
                code_id,
            }) = event
            {
                let mirror_query = MirrorQuery::from_provider(
                    Address::from_word(actor_id.into_bytes().into()),
                    provider.clone(),
                );
                let state_hash = mirror_query.state_hash().await?;
                states.push((actor_id, code_id, state_hash));
            }
        }
        handled_blocks.push((block, states));
    }

    Ok(handled_blocks)
}

async fn process_response_validation(
    validating_response: &db_sync::ValidatingResponse,
    router_query: &mut RouterQuery,
) -> Result<bool> {
    let response = validating_response.response();

    if let db_sync::Response::ProgramIds(ids) = response {
        let ethereum_programs = router_query.programs_count().await?;
        if ethereum_programs != U256::from(ids.len()) {
            return Ok(false);
        }

        let ids: Vec<ActorId> = ids.iter().copied().collect();
        let ids_len = ids.len();
        let code_ids = router_query.programs_code_ids(ids).await?;
        if code_ids.len() != ids_len {
            return Ok(false);
        }
    }

    Ok(true)
}
