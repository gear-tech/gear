// This file is part of Gear.
//
// Copyright (C) 2024 Gear Technologies Inc.
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

use crate::{
    db_sync::{InnerBehaviour, Request, RequestFailure, RequestId, Response},
    export::PeerId,
};
use ethexe_db::{CodesStorage, Database};
use libp2p::{
    request_response,
    request_response::OutboundRequestId,
    swarm::{behaviour::ConnectionEstablished, ConnectionClosed, ConnectionId, FromSwarm},
};
use rand::seq::IteratorRandom;
use std::{
    collections::{HashMap, HashSet},
    task::{Context, Poll},
};
use tokio::task::JoinSet;

pub(crate) enum OngoingRequestCompletion {
    Full(Response),
    Partial(OngoingRequest),
}

#[derive(Debug)]
pub(crate) struct OngoingRequest {
    request_id: RequestId,
    request: Request,
    response: Option<Response>,
    tried_peers: HashSet<PeerId>,
}

impl OngoingRequest {
    pub(crate) fn new(request_id: RequestId, request: Request) -> Self {
        Self {
            request_id,
            request,
            response: None,
            tried_peers: HashSet::new(),
        }
    }

    pub(crate) fn request_id(&self) -> RequestId {
        self.request_id
    }

    pub(crate) fn into_request(self) -> Request {
        self.request
    }
    pub(crate) fn into_response(self) -> Option<Response> {
        self.response
    }

    pub(crate) fn into_response_or_request(self) -> Result<Response, Request> {
        self.response.ok_or(self.request)
    }

    fn merge_response(&mut self, new_response: Response) -> Response {
        if let Some(response) = self.response.take() {
            response.merge(new_response)
        } else {
            new_response
        }
    }

    /// Try to bring request to the complete state.
    ///
    /// Returns error if response validation is failed.
    pub(crate) fn try_complete(
        mut self,
        peer: PeerId,
        response: Response,
    ) -> Result<OngoingRequestCompletion, RequestFailure> {
        self.request.validate_response(&response)?;

        if let Some(new_request) = self.request.difference(&response) {
            self.request = new_request;
            self.tried_peers.insert(peer);
            self.response = Some(self.merge_response(response));
            Ok(OngoingRequestCompletion::Partial(self))
        } else {
            let response = self.merge_response(response);
            Ok(OngoingRequestCompletion::Full(response))
        }
    }

    /// Peer failed to handle request, so we create new ongoing request for the next round.
    pub(crate) fn peer_failed(mut self, peer: PeerId) -> Self {
        self.tried_peers.insert(peer);
        self
    }

    fn choose_next_peer(
        &mut self,
        peers: &HashMap<PeerId, HashSet<ConnectionId>>,
    ) -> Option<PeerId> {
        let peers: HashSet<PeerId> = peers.keys().copied().collect();
        let peer = peers
            .difference(&self.tried_peers)
            .choose_stable(&mut rand::thread_rng())
            .copied();
        peer
    }
}

#[derive(Debug, Default)]
pub(crate) struct OngoingRequests {
    inner: HashMap<OutboundRequestId, OngoingRequest>,
    connections: HashMap<PeerId, HashSet<ConnectionId>>,
}

impl OngoingRequests {
    /// Tracks all active connections.
    pub(crate) fn on_swarm_event(&mut self, event: FromSwarm) {
        match event {
            FromSwarm::ConnectionEstablished(ConnectionEstablished {
                peer_id,
                connection_id,
                ..
            }) => {
                self.connections
                    .entry(peer_id)
                    .or_default()
                    .insert(connection_id);
            }
            FromSwarm::ConnectionClosed(ConnectionClosed {
                peer_id,
                connection_id,
                ..
            }) => {
                self.connections
                    .entry(peer_id)
                    .or_default()
                    .remove(&connection_id);
            }
            _ => {}
        }
    }

    pub(crate) fn remove(
        &mut self,
        outbound_request_id: OutboundRequestId,
    ) -> Option<OngoingRequest> {
        self.inner.remove(&outbound_request_id)
    }

    /// Send actual request to behaviour and tracks its state.
    ///
    /// On success, returns peer ID we sent request to.
    ///
    /// On error, returns request back if no peer connected to the swarm.
    pub(crate) fn send_request(
        &mut self,
        behaviour: &mut InnerBehaviour,
        mut ongoing_request: OngoingRequest,
    ) -> Result<PeerId, OngoingRequest> {
        let peer_id = ongoing_request.choose_next_peer(&self.connections);
        if let Some(peer_id) = peer_id {
            let outbound_request_id =
                behaviour.send_request(&peer_id, ongoing_request.request.clone());

            self.inner.insert(outbound_request_id, ongoing_request);

            Ok(peer_id)
        } else {
            Err(ongoing_request)
        }
    }
}

pub(crate) struct OngoingResponses {
    db: Database,
    db_readers: JoinSet<(request_response::ResponseChannel<Response>, Response)>,
}

impl OngoingResponses {
    pub(crate) fn from_db(db: Database) -> Self {
        Self {
            db,
            db_readers: JoinSet::new(),
        }
    }

    pub(crate) fn prepare_response(
        &mut self,
        channel: request_response::ResponseChannel<Response>,
        request: Request,
    ) {
        let db = self.db.clone();
        self.db_readers.spawn_blocking(move || {
            let response = match request {
                Request::DataForHashes(hashes) => Response::DataForHashes(
                    hashes
                        .into_iter()
                        .filter_map(|hash| Some((hash, db.read_by_hash(hash)?)))
                        .collect(),
                ),
                Request::ProgramIds => Response::ProgramIds(db.program_ids()),
            };
            (channel, response)
        });
    }

    fn poll_inner_next(
        &mut self,
        cx: &mut Context<'_>,
    ) -> Poll<(request_response::ResponseChannel<Response>, Response)> {
        match self.db_readers.poll_join_next(cx) {
            Poll::Ready(Some(res)) => {
                let values = res.expect("database panicked");
                Poll::Ready(values)
            }
            Poll::Ready(None) => Poll::Pending,
            Poll::Pending => Poll::Pending,
        }
    }

    pub(crate) fn poll_send_response(
        &mut self,
        cx: &mut Context<'_>,
        behaviour: &mut InnerBehaviour,
    ) {
        if let Poll::Ready((channel, response)) = self.poll_inner_next(cx) {
            let _res = behaviour.send_response(channel, response);
        }
    }
}
