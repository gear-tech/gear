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
use ethexe_common::{db::BlockMetaStorage, events::BlockRequestEvent};
use ethexe_db::Database;
use ethexe_ethereum::{
    mirror::MirrorQuery,
    primitives::{Address, U256},
    router::RouterQuery,
};
use ethexe_network::{db_sync, db_sync::RequestId, NetworkEvent, NetworkService};
use ethexe_observer::Query;
use ethexe_runtime_common::state::{
    ActiveProgram, HashOf, MaybeHashOf, MemoryPages, MemoryPagesRegion, Program, ProgramState,
    Storage,
};
use futures::StreamExt;
use gear_core::ids::ProgramId;
use gprimitives::H256;
use parity_scale_codec::Decode;
use std::{
    collections::{BTreeSet, HashMap},
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
struct Requests {
    /// Buffered hashes we sent after all of them are collected
    buffered_request: BTreeSet<H256>,
    /// Buffered requests kinds we drain into `pending_requests` when `RequestId` is known
    buffered_kinds: Vec<(RequestKind, H256)>,
    /// Pending requests, we remove one by one on each hash from response
    pending_requests: HashMap<(RequestId, H256), RequestKind>,
}

impl Requests {
    fn add(&mut self, request: RequestKind, hashes: impl IntoIterator<Item = H256>) {
        for hash in hashes {
            self.buffered_request.insert(hash);
            self.buffered_kinds.push((request, hash));
        }
    }

    fn request(&mut self, network: &mut NetworkService) {
        let buffered_request = mem::take(&mut self.buffered_request);
        let request_id = network.request_db_data(db_sync::Request::DataForHashes(buffered_request));
        for (request, hash) in self.buffered_kinds.drain(..) {
            self.pending_requests.insert((request_id, hash), request);
        }
    }

    fn remove(&mut self, request_id: RequestId, hash: H256) -> Option<RequestKind> {
        self.pending_requests.remove(&(request_id, hash))
    }

    fn is_empty(&self) -> bool {
        self.pending_requests.is_empty()
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

    let programs_states =
        collect_programs_states(router_query, query, db, observer.provider().root().clone())
            .await?;
    log::error!("Program IDs from mirrors: {programs_states:?}");

    let mut requests = Requests::default();
    requests.add(
        RequestKind::ProgramState,
        programs_states
            .into_iter()
            .map(|(_program_id, states)| states)
            .flatten(),
    );
    requests.request(network);

    while let Some(event) = network.next().await {
        match event {
            NetworkEvent::DbResponse { request_id, result } => match result {
                Ok(db_sync::Response::DataForHashes(data)) => {
                    for (hash, data) in data {
                        let request = requests
                            .remove(request_id, hash)
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
                                log::error!("New data: {data:?}");
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

        requests.request(network);

        if requests.is_empty() {
            break;
        }
    }

    log::info!("Fast synchronization done");

    Ok(())
}

async fn collect_programs_states(
    router_query: &RouterQuery,
    query: &mut Query,
    db: &Database,
    provider: RootProvider<BoxTransport>,
) -> Result<HashMap<ProgramId, BTreeSet<H256>>> {
    let latest_block = router_query.latest_committed_block_hash().await?;
    let blocks = query.get_last_committed_chain(latest_block).await?;

    let mut program_states = HashMap::<ProgramId, BTreeSet<H256>>::new();
    for block in blocks {
        let events = db
            .block_events(block)
            .context("`get_last_committed_chain` must insert block events")?;

        for event in events {
            match event {
                BlockRequestEvent::Router(_) => {}
                BlockRequestEvent::Mirror { actor_id, event: _ } => {
                    let mirror_query = MirrorQuery::from_provider(
                        Address::from_word(actor_id.into_bytes().into()),
                        provider.clone(),
                    );
                    let state_hash = mirror_query.state_hash().await?;
                    program_states
                        .entry(actor_id)
                        .or_default()
                        .insert(state_hash);
                }
                BlockRequestEvent::WVara(_) => {}
            }
        }
    }

    Ok(program_states)
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

        let ids: Vec<ProgramId> = ids.iter().copied().collect();
        let ids_len = ids.len();
        let code_ids = router_query.programs_code_ids(ids).await?;
        if code_ids.len() != ids_len {
            return Ok(false);
        }
    }

    Ok(true)
}
