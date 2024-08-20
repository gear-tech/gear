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
    db_sync::{Config, InnerBehaviour, Request, RequestId, Response, ResponseId},
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
    collections::{HashMap, HashSet, VecDeque},
    future::Future,
    pin::Pin,
    task::{Context, Poll},
    time::Duration,
};
use tokio::{task::JoinSet, time, time::Sleep};

#[derive(Debug)]
pub(crate) struct SendRequestError {
    pub(crate) request_id: RequestId,
    pub(crate) kind: SendRequestErrorKind,
}

#[derive(Debug)]
pub(crate) enum SendRequestErrorKind {
    OutOfRounds,
    Pending,
}

#[derive(Debug, derive_more::From)]
pub(crate) enum PeerResponse {
    NewRound {
        peer_id: PeerId,
        request_id: RequestId,
    },
    #[from]
    SendRequest(SendRequestError),
}

#[derive(Debug, derive_more::From)]
pub(crate) enum PeerFailed {
    #[from]
    SendRequest(SendRequestError),
}

#[derive(Debug)]
pub(crate) struct OngoingRequest {
    request_id: RequestId,
    request: Request,
    response: Option<Response>,
    tried_peers: HashSet<PeerId>,
    timeout: Pin<Box<Sleep>>,
}

impl OngoingRequest {
    pub(crate) fn new(request_id: RequestId, request: Request, timeout: Duration) -> Self {
        Self {
            request_id,
            request,
            response: None,
            tried_peers: HashSet::new(),
            timeout: Box::pin(time::sleep(timeout)),
        }
    }

    fn merge_response(&mut self, new_response: Response) -> Response {
        if let Some(response) = self.response.take() {
            response.merge(new_response)
        } else {
            new_response
        }
    }

    /// Try to bring the request to the complete state.
    ///
    /// Returns error if response validation is failed or response is incomplete.
    fn try_complete(mut self, peer: PeerId, response: Response) -> Result<Response, Self> {
        self.tried_peers.insert(peer);

        if let Err(error) = self.request.validate_response(&response) {
            let request_id = self.request_id;
            log::trace!(
                "response validation failed for request {request_id:?} from {peer}: {error:?}",
            );
            Err(self)
        } else if let Some(new_request) = self.request.difference(&response) {
            self.request = new_request;
            self.response = Some(self.merge_response(response));
            Err(self)
        } else {
            let response = self.merge_response(response);
            Ok(response)
        }
    }

    /// Peer failed to handle the request, so we create a new ongoing request for the next round.
    fn peer_failed(mut self, peer: PeerId) -> Self {
        self.tried_peers.insert(peer);
        self
    }

    fn choose_next_peer(
        &mut self,
        peers: &HashMap<PeerId, HashSet<ConnectionId>>,
        max_rounds_per_request: u32,
    ) -> Result<Option<PeerId>, SendRequestError> {
        if self.tried_peers.len() >= max_rounds_per_request as usize {
            return Err(SendRequestError {
                request_id: self.request_id,
                kind: SendRequestErrorKind::OutOfRounds,
            });
        }

        let peers: HashSet<PeerId> = peers.keys().copied().collect();
        let peer = peers
            .difference(&self.tried_peers)
            .choose_stable(&mut rand::thread_rng())
            .copied();
        Ok(peer)
    }
}

#[derive(Debug)]
pub(crate) struct OngoingRequests {
    connections: HashMap<PeerId, HashSet<ConnectionId>>,
    request_id_counter: u64,
    pending_requests: VecDeque<OngoingRequest>,
    active_requests: HashMap<OutboundRequestId, OngoingRequest>,
    max_rounds_per_request: u32,
    request_timeout: Duration,
}

impl OngoingRequests {
    pub(crate) fn from_config(config: &Config) -> Self {
        Self {
            connections: Default::default(),
            request_id_counter: 0,
            pending_requests: Default::default(),
            active_requests: Default::default(),
            max_rounds_per_request: config.max_rounds_per_request,
            request_timeout: config.request_timeout,
        }
    }

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

    fn next_request_id(&mut self) -> RequestId {
        self.request_id_counter += 1;
        RequestId(self.request_id_counter)
    }

    pub(crate) fn push_pending_request(&mut self, request: Request) -> RequestId {
        let request_id = self.next_request_id();
        let ongoing_request = OngoingRequest::new(request_id, request, self.request_timeout);
        self.pending_requests.push_front(ongoing_request);
        request_id
    }

    pub(crate) fn remove_if_timeout(&mut self, cx: &mut Context<'_>) -> Option<RequestId> {
        let outbound_request_id =
            self.active_requests
                .iter_mut()
                .find_map(|(&request_id, active_request)| {
                    if active_request.timeout.as_mut().poll(cx).is_ready() {
                        Some(request_id)
                    } else {
                        None
                    }
                })?;

        let outgoing_request = self
            .active_requests
            .remove(&outbound_request_id)
            .expect("infallible");
        Some(outgoing_request.request_id)
    }

    fn send_request(
        &mut self,
        behaviour: &mut InnerBehaviour,
        mut ongoing_request: OngoingRequest,
    ) -> Result<PeerId, SendRequestError> {
        let peer_id =
            ongoing_request.choose_next_peer(&self.connections, self.max_rounds_per_request)?;
        if let Some(peer_id) = peer_id {
            let outbound_request_id =
                behaviour.send_request(&peer_id, ongoing_request.request.clone());

            self.active_requests
                .insert(outbound_request_id, ongoing_request);

            Ok(peer_id)
        } else {
            let request_id = ongoing_request.request_id;
            self.pending_requests.push_back(ongoing_request);
            Err(SendRequestError {
                request_id,
                kind: SendRequestErrorKind::Pending,
            })
        }
    }

    pub(crate) fn send_pending_request(
        &mut self,
        behaviour: &mut InnerBehaviour,
    ) -> Result<Option<(PeerId, RequestId)>, SendRequestError> {
        let Some(ongoing_request) = self.pending_requests.pop_back() else {
            return Ok(None);
        };

        let request_id = ongoing_request.request_id;
        let peer_id = self.send_request(behaviour, ongoing_request)?;
        Ok(Some((peer_id, request_id)))
    }

    pub(crate) fn on_peer_response(
        &mut self,
        behaviour: &mut InnerBehaviour,
        peer: PeerId,
        request_id: OutboundRequestId,
        response: Response,
    ) -> Result<(RequestId, Response), PeerResponse> {
        let ongoing_request = self
            .active_requests
            .remove(&request_id)
            .expect("unknown response");
        let request_id = ongoing_request.request_id;

        let new_ongoing_request = match ongoing_request.try_complete(peer, response) {
            Ok(response) => return Ok((request_id, response)),
            Err(new_ongoing_request) => new_ongoing_request,
        };

        let peer_id = self.send_request(behaviour, new_ongoing_request)?;
        Err(PeerResponse::NewRound {
            peer_id,
            request_id,
        })
    }

    pub(crate) fn on_peer_failed(
        &mut self,
        behaviour: &mut InnerBehaviour,
        peer: PeerId,
        request_id: OutboundRequestId,
    ) -> Result<(PeerId, RequestId), PeerFailed> {
        let ongoing_request = self
            .active_requests
            .remove(&request_id)
            .expect("unknown response");
        let request_id = ongoing_request.request_id;
        let new_ongoing_request = ongoing_request.peer_failed(peer);
        let peer_id = self.send_request(behaviour, new_ongoing_request)?;
        Ok((peer_id, request_id))
    }
}

struct OngoingResponse {
    response_id: ResponseId,
    peer_id: PeerId,
    channel: request_response::ResponseChannel<Response>,
    response: Response,
}

pub(crate) struct OngoingResponses {
    response_id_counter: u64,
    db: Database,
    db_readers: JoinSet<OngoingResponse>,
    max_simultaneous_responses: u32,
}

impl OngoingResponses {
    pub(crate) fn new(db: Database, config: &Config) -> Self {
        Self {
            response_id_counter: 0,
            db,
            db_readers: JoinSet::new(),
            max_simultaneous_responses: config.max_simultaneous_responses,
        }
    }

    fn next_response_id(&mut self) -> ResponseId {
        self.response_id_counter += 1;
        ResponseId(self.response_id_counter)
    }

    pub(crate) fn prepare_response(
        &mut self,
        peer_id: PeerId,
        channel: request_response::ResponseChannel<Response>,
        request: Request,
    ) -> Option<ResponseId> {
        if self.db_readers.len() >= self.max_simultaneous_responses as usize {
            return None;
        }

        let response_id = self.next_response_id();

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

            OngoingResponse {
                response_id,
                peer_id,
                channel,
                response,
            }
        });

        Some(response_id)
    }

    pub(crate) fn poll_send_response(
        &mut self,
        cx: &mut Context<'_>,
        behaviour: &mut InnerBehaviour,
    ) -> Poll<(PeerId, ResponseId)> {
        if let Poll::Ready(Some(res)) = self.db_readers.poll_join_next(cx) {
            let OngoingResponse {
                response_id,
                peer_id,
                channel,
                response,
            } = res.expect("database panicked");
            let _res = behaviour.send_response(channel, response);
            Poll::Ready((peer_id, response_id))
        } else {
            Poll::Pending
        }
    }
}
