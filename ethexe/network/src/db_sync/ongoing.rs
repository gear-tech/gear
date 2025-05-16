// This file is part of Gear.
//
// Copyright (C) 2024-2025 Gear Technologies Inc.
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
    peer_score::Handle,
    utils::ConnectionMap,
};
use ethexe_db::Database;
use ethexe_service_utils::Timer;
use futures::FutureExt;
use libp2p::{
    request_response,
    request_response::OutboundRequestId,
    swarm::{behaviour::ConnectionEstablished, ConnectionClosed, FromSwarm},
};
use rand::seq::IteratorRandom;
use std::{
    collections::{HashMap, HashSet, VecDeque},
    task::{Context, Poll},
    time::Duration,
};
use tokio::task::JoinSet;

#[derive(Debug)]
pub(crate) struct SendNextRequest {
    pub(crate) peer_id: PeerId,
    pub(crate) request_id: RequestId,
}

#[derive(Debug)]
pub(crate) enum SendRequestError {
    OutOfRounds(OngoingRequest),
    NoPeers(RequestId),
}

#[derive(Debug)]
pub(crate) enum PeerResponse {
    Success {
        request_id: RequestId,
        response: Response,
    },
    NewRound {
        peer_id: PeerId,
        request_id: RequestId,
    },
}

#[derive(Debug)]
pub(crate) struct PeerFailed {
    pub(crate) peer_id: PeerId,
    pub(crate) request_id: RequestId,
}

#[derive(Debug, Clone)]
pub struct OngoingRequest {
    request_id: RequestId,
    original_request: Request,
    request: Request,
    response: Option<Response>,
    tried_peers: HashSet<PeerId>,
    timer: Timer,
    peer_score_handle: Handle,
}

impl PartialEq for OngoingRequest {
    fn eq(&self, other: &Self) -> bool {
        self.request_id == other.request_id
    }
}

impl Eq for OngoingRequest {}

impl OngoingRequest {
    pub(crate) fn new(
        request_id: RequestId,
        request: Request,
        timeout: Duration,
        peer_score_handle: Handle,
    ) -> Self {
        Self {
            request_id,
            original_request: request.clone(),
            request,
            response: None,
            tried_peers: HashSet::new(),
            timer: Timer::new("ongoing-request", timeout),
            peer_score_handle,
        }
    }

    pub(crate) fn id(&self) -> RequestId {
        self.request_id
    }

    fn merge_and_strip(&mut self, peer: PeerId, new_response: Response) -> Response {
        let mut response = if let Some(mut response) = self.response.take() {
            response.merge(new_response);
            response
        } else {
            new_response
        };

        if response.strip(&self.original_request) {
            log::debug!(
                "data stripped in response from {peer} for {:?}",
                self.request_id
            );
            self.peer_score_handle.excessive_data(peer);
        }

        response
    }

    #[allow(clippy::result_large_err)]
    fn inner_complete(
        mut self,
        peer: PeerId,
        response: Response,
    ) -> Result<(RequestId, Response), Self> {
        if let Some(new_request) = self.request.difference(&response) {
            self.request = new_request;
            self.response = Some(self.merge_and_strip(peer, response));
            Err(self)
        } else {
            let request_id = self.request_id;
            let response = self.merge_and_strip(peer, response);
            Ok((request_id, response))
        }
    }

    /// Try to bring the request to the complete state.
    ///
    /// Returns `Err(self)` if response validation is failed or response is incomplete.
    #[allow(clippy::result_large_err)]
    fn try_complete(mut self, peer: PeerId, response: Response) -> Result<PeerResponse, Self> {
        self.tried_peers.insert(peer);

        let request_id = self.request_id;

        match response.validate() {
            Ok(()) => self
                .inner_complete(peer, response)
                .map(|(request_id, response)| PeerResponse::Success {
                    request_id,
                    response,
                }),
            Err(error) => {
                log::trace!(
                    "response validation failed for request {request_id:?} from {peer}: {error:?}",
                );
                self.peer_score_handle.invalid_data(peer);

                Err(self)
            }
        }
    }

    /// Peer failed to handle the request, so we create a new ongoing request for the next round.
    fn peer_failed(mut self, peer: PeerId) -> Self {
        self.tried_peers.insert(peer);
        self
    }

    #[allow(clippy::result_large_err)]
    fn choose_next_peer(
        self,
        map: &ConnectionMap,
        max_rounds_per_request: u32,
    ) -> Result<(Self, Option<PeerId>), SendRequestError> {
        if self.tried_peers.len() >= max_rounds_per_request as usize {
            return Err(SendRequestError::OutOfRounds(self));
        }

        let peers: HashSet<PeerId> = map.peers().collect();
        let peer = peers
            .difference(&self.tried_peers)
            .choose_stable(&mut rand::thread_rng())
            .copied();
        Ok((self, peer))
    }
}

#[derive(Debug)]
pub(crate) struct OngoingRequests {
    connections: ConnectionMap,
    request_id_counter: u64,
    pending_requests: VecDeque<OngoingRequest>,
    active_requests: HashMap<OutboundRequestId, OngoingRequest>,
    /// Requests that have been removed before `InnerBehaviour` returned event
    beforehand_removed_requests: HashSet<OutboundRequestId>,
    max_rounds_per_request: u32,
    request_timeout: Duration,
    peer_score_handle: Handle,
}

impl OngoingRequests {
    pub(crate) fn new(config: &Config, peer_score_handle: Handle) -> Self {
        Self {
            connections: Default::default(),
            request_id_counter: 0,
            pending_requests: Default::default(),
            active_requests: Default::default(),
            beforehand_removed_requests: Default::default(),
            max_rounds_per_request: config.max_rounds_per_request,
            request_timeout: config.request_timeout,
            peer_score_handle,
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
                let res = self.connections.add_connection(peer_id, connection_id);
                debug_assert_eq!(res, Ok(()));
            }
            FromSwarm::ConnectionClosed(ConnectionClosed {
                peer_id,
                connection_id,
                ..
            }) => {
                self.connections.remove_connection(peer_id, connection_id);
            }
            _ => {}
        }
    }

    fn next_request_id(&mut self) -> RequestId {
        let id = self.request_id_counter;
        self.request_id_counter += 1;
        RequestId(id)
    }

    fn remove_active_request(&mut self, request_id: OutboundRequestId) -> Option<OngoingRequest> {
        let ongoing_request = self.active_requests.remove(&request_id);
        if ongoing_request.is_none() {
            assert!(
                self.beforehand_removed_requests.remove(&request_id),
                "unknown request: {request_id:?}"
            );
        }
        ongoing_request
    }

    pub(crate) fn push_pending_request(&mut self, request: Request) -> RequestId {
        let request_id = self.next_request_id();
        let ongoing_request = OngoingRequest::new(
            request_id,
            request,
            self.request_timeout,
            self.peer_score_handle.clone(),
        );
        self.pending_requests.push_front(ongoing_request);
        request_id
    }

    pub(crate) fn retry(&mut self, ongoing_request: OngoingRequest) {
        self.pending_requests.push_front(ongoing_request);
    }

    pub(crate) fn remove_if_timeout(&mut self, cx: &mut Context<'_>) -> Option<OngoingRequest> {
        let outbound_request_id =
            self.active_requests
                .iter_mut()
                .find_map(|(&request_id, active_request)| {
                    if active_request.timer.poll_unpin(cx).is_ready() {
                        Some(request_id)
                    } else {
                        None
                    }
                })?;

        let outgoing_request = self
            .active_requests
            .remove(&outbound_request_id)
            .expect("infallible");
        self.beforehand_removed_requests.insert(outbound_request_id);

        Some(outgoing_request)
    }

    #[allow(clippy::result_large_err)]
    fn send_request(
        &mut self,
        behaviour: &mut InnerBehaviour,
        ongoing_request: OngoingRequest,
    ) -> Result<PeerId, SendRequestError> {
        let (ongoing_request, peer_id) =
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
            Err(SendRequestError::NoPeers(request_id))
        }
    }

    #[allow(clippy::result_large_err)]
    pub(crate) fn send_next_request(
        &mut self,
        behaviour: &mut InnerBehaviour,
    ) -> Result<Option<SendNextRequest>, SendRequestError> {
        let Some(mut ongoing_request) = self.pending_requests.pop_back() else {
            return Ok(None);
        };
        ongoing_request.timer.start(());

        let request_id = ongoing_request.request_id;
        let peer_id = self.send_request(behaviour, ongoing_request)?;
        Ok(Some(SendNextRequest {
            request_id,
            peer_id,
        }))
    }

    #[allow(clippy::result_large_err)]
    pub(crate) fn on_peer_response(
        &mut self,
        behaviour: &mut InnerBehaviour,
        peer: PeerId,
        request_id: OutboundRequestId,
        response: Response,
    ) -> Result<Option<PeerResponse>, SendRequestError> {
        let Some(ongoing_request) = self.remove_active_request(request_id) else {
            return Ok(None);
        };
        let request_id = ongoing_request.request_id;

        let new_ongoing_request = match ongoing_request.try_complete(peer, response) {
            Ok(peer_response) => return Ok(Some(peer_response)),
            Err(new_ongoing_request) => new_ongoing_request,
        };

        let peer_id = self.send_request(behaviour, new_ongoing_request)?;
        Ok(Some(PeerResponse::NewRound {
            peer_id,
            request_id,
        }))
    }

    #[allow(clippy::result_large_err)]
    pub(crate) fn on_peer_failed(
        &mut self,
        behaviour: &mut InnerBehaviour,
        peer: PeerId,
        request_id: OutboundRequestId,
    ) -> Result<Option<PeerFailed>, SendRequestError> {
        let Some(ongoing_request) = self.remove_active_request(request_id) else {
            return Ok(None);
        };
        let request_id = ongoing_request.request_id;
        let new_ongoing_request = ongoing_request.peer_failed(peer);
        let peer_id = self.send_request(behaviour, new_ongoing_request)?;
        Ok(Some(PeerFailed {
            peer_id,
            request_id,
        }))
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
        let id = self.response_id_counter;
        self.response_id_counter += 1;
        ResponseId(id)
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
            let response = Response::from_db(request, &db);
            OngoingResponse {
                response_id,
                peer_id,
                channel,
                response,
            }
        });

        Some(response_id)
    }

    pub(crate) fn poll(
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
