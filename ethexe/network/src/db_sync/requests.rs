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

use crate::{
    db_sync::{
        Config, Event, InnerBehaviour, NewRequestRoundReason, PeerId, Request, RequestFailure,
        RequestId, Response,
    },
    peer_score::Handle,
    utils::ConnectionMap,
};
use ethexe_service_utils::Timer;
use futures::FutureExt;
use libp2p::{
    request_response::OutboundRequestId,
    swarm::{behaviour::ConnectionEstablished, ConnectionClosed, FromSwarm},
};
use rand::prelude::IteratorRandom;
use std::{
    collections::{HashMap, HashSet, VecDeque},
    task::{Context, Poll},
    time::Duration,
};
use tokio::sync::{
    oneshot,
    oneshot::{Receiver, Sender},
};

pub(crate) struct OngoingRequests {
    requests: HashMap<RequestId, OngoingRequest>,
    active_requests: HashMap<OutboundRequestId, RequestId>,
    connections: ConnectionMap,
    request_id_counter: u64,
    peer_score_handle: Handle,
    request_timeout: Duration,
    max_rounds_per_request: u32,
}

impl OngoingRequests {
    pub(crate) fn new(config: &Config, peer_score_handle: Handle) -> Self {
        Self {
            requests: Default::default(),
            active_requests: Default::default(),
            connections: Default::default(),
            request_id_counter: 0,
            peer_score_handle,
            request_timeout: config.request_timeout,
            max_rounds_per_request: config.max_rounds_per_request,
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

    pub(crate) fn request(&mut self, request: Request) -> RequestId {
        let request_id = self.next_request_id();
        let request = OngoingRequest {
            state: Some(State::SendToNextPeer(NewRequestRoundReason::FromQueue)),
            timer: Timer::new("ongoing request", self.request_timeout),
            peer_score_handle: self.peer_score_handle.clone(),

            request: request.clone(),
            partial_response: None,
            original_request: request,
            tried_peers: HashSet::new(),
        };
        self.requests.insert(request_id, request);
        request_id
    }

    pub(crate) fn retry(&mut self, request: RetriableRequest) {
        let RetriableRequest {
            request_id,
            request,
            partial_response,
            tried_peers,
            original_request,
        } = request;
        self.requests.insert(
            request_id,
            OngoingRequest {
                state: Some(State::SendToNextPeer(NewRequestRoundReason::FromQueue)),
                timer: Timer::new("ongoing request", self.request_timeout),
                peer_score_handle: self.peer_score_handle.clone(),

                request,
                partial_response,
                original_request,
                tried_peers,
            },
        );
    }

    pub(crate) fn on_peer_response(
        &mut self,
        peer: PeerId,
        outbound_request_id: OutboundRequestId,
        response: Response,
    ) {
        let request_id = self
            .active_requests
            .remove(&outbound_request_id)
            .expect("unknown outbound request id");
        let request = self.requests.get_mut(&request_id);
        if let Some(request) = request {
            request.on_peer_response(peer, response);
        } else {
            log::trace!("request {outbound_request_id} has been skipped");
        }
    }

    pub(crate) fn on_peer_failure(&mut self, outbound_request_id: OutboundRequestId) {
        let request_id = self
            .active_requests
            .remove(&outbound_request_id)
            .expect("unknown outbound request id");
        let request = self.requests.get_mut(&request_id);
        if let Some(request) = request {
            request.on_peer_failure();
        } else {
            log::trace!("request {outbound_request_id} has been skipped");
        }
    }

    pub(crate) fn poll_next_states(
        &mut self,
        cx: &mut Context<'_>,
        behaviour: &mut InnerBehaviour,
    ) -> Vec<Event> {
        let mut events = Vec::new();
        let mut kept = Vec::new();

        for (request_id, mut request) in self.requests.drain() {
            let mut ctx = OngoingRequestContext {
                task_cx: cx,
                pending_events: VecDeque::new(),
                connections: &self.connections,
                max_rounds_per_request: self.max_rounds_per_request as usize,
            };

            let poll = request.poll(&mut ctx);

            for event in ctx.pending_events {
                match event {
                    OngoingRequestEvent::PendingState => {
                        events.push(Event::PendingStateRequest { request_id });
                    }
                    OngoingRequestEvent::SendRequest(peer, request, reason) => {
                        let outbound_request_id = behaviour.send_request(&peer, request);
                        self.active_requests.insert(outbound_request_id, request_id);

                        events.push(Event::NewRequestRound {
                            request_id,
                            peer_id: peer,
                            reason,
                        });
                    }
                    OngoingRequestEvent::ExternalValidationRequired(sender, response) => {
                        events.push(Event::ExternalValidationRequired {
                            request_id,
                            response,
                            sender,
                        });
                    }
                }
            }

            match poll {
                Poll::Ready(Ok(response)) => events.push(Event::RequestSucceed {
                    request_id,
                    response,
                }),
                Poll::Ready(Err(error)) => events.push(Event::RequestFailed {
                    request: RetriableRequest::new(request_id, request),
                    error,
                }),
                Poll::Pending => kept.push((request_id, request)),
            }
        }

        self.requests.extend(kept);

        events
    }
}

#[derive(Debug)]
enum OngoingRequestEvent {
    PendingState,
    SendRequest(PeerId, Request, NewRequestRoundReason),
    ExternalValidationRequired(Sender<bool>, Response),
}

struct OngoingRequestContext<'a, 'cx> {
    task_cx: &'a mut Context<'cx>,
    pending_events: VecDeque<OngoingRequestEvent>,
    connections: &'a ConnectionMap,
    max_rounds_per_request: usize,
}

#[derive(Debug)]
enum State {
    SendToNextPeer(NewRequestRoundReason),
    AwaitingResponse,
    OnPeerResponse(PeerId, Response),
    OnPeerFailure,
    MergeAndStrip(PeerId, Response),
    AwaitingExternalValidation(PeerId, Receiver<bool>, Response),
}

struct OngoingRequest {
    // future state
    state: Option<State>,
    timer: Timer,
    peer_score_handle: Handle,

    // common state
    request: Request,
    partial_response: Option<Response>,
    original_request: Request,
    tried_peers: HashSet<PeerId>,
}

impl OngoingRequest {
    fn choose_next_peer(&mut self, connections: &ConnectionMap) -> Option<PeerId> {
        let peers: HashSet<PeerId> = connections.peers().collect();
        let peer = peers
            .difference(&self.tried_peers)
            .choose_stable(&mut rand::thread_rng())
            .copied();
        self.tried_peers.extend(peer);
        peer
    }

    fn merge_and_strip(&mut self, peer: PeerId, new_response: Response) -> Response {
        let mut response = if let Some(mut response) = self.partial_response.take() {
            response.merge(new_response);
            response
        } else {
            new_response
        };

        if response.strip(&self.original_request) {
            log::debug!("data stripped in response from {peer}");
            self.peer_score_handle.excessive_data(peer);
        }

        response
    }

    fn on_peer_response(&mut self, peer: PeerId, response: Response) {
        if let Some(State::AwaitingResponse) = self.state {
            self.state = Some(State::OnPeerResponse(peer, response));
        } else {
            unreachable!();
        }
    }

    fn on_peer_failure(&mut self) {
        if let Some(State::AwaitingResponse) = self.state {
            self.state = Some(State::OnPeerFailure);
        } else {
            unreachable!();
        }
    }

    fn poll(&mut self, ctx: &mut OngoingRequestContext) -> Poll<Result<Response, RequestFailure>> {
        if self.timer.poll_unpin(ctx.task_cx).is_ready() {
            return Poll::Ready(Err(RequestFailure::Timeout));
        }

        // TODO: after retry the branch can be always true
        if self.tried_peers.len() > ctx.max_rounds_per_request {
            return Poll::Ready(Err(RequestFailure::OutOfRounds));
        }

        loop {
            let mut pending = false;
            let next_state = match self.state.take().expect("always Some") {
                State::SendToNextPeer(reason) => {
                    if let Some(peer) = self.choose_next_peer(ctx.connections) {
                        // FIXME: reactivated each time
                        self.timer.start(());
                        ctx.pending_events
                            .push_back(OngoingRequestEvent::SendRequest(
                                peer,
                                self.request.clone(),
                                reason,
                            ));
                        State::AwaitingResponse
                    } else {
                        pending = true;
                        ctx.pending_events
                            .push_back(OngoingRequestEvent::PendingState);
                        State::SendToNextPeer(reason)
                    }
                }
                State::AwaitingResponse => {
                    pending = true;
                    State::AwaitingResponse
                }
                State::OnPeerResponse(peer, response) => match response.validate() {
                    Ok(true) => State::MergeAndStrip(peer, response),
                    Ok(false) => {
                        let (sender, receiver) = oneshot::channel();
                        ctx.pending_events.push_back(
                            OngoingRequestEvent::ExternalValidationRequired(
                                sender,
                                response.clone(),
                            ),
                        );
                        State::AwaitingExternalValidation(peer, receiver, response)
                    }
                    Err(err) => {
                        log::trace!("response validation failed for request from {peer}: {err:?}");
                        self.peer_score_handle.invalid_data(peer);
                        State::SendToNextPeer(NewRequestRoundReason::PartialData)
                    }
                },
                State::OnPeerFailure => State::SendToNextPeer(NewRequestRoundReason::PeerFailed),
                State::MergeAndStrip(peer, response) => {
                    if let Some(new_request) = self.request.difference(&response) {
                        self.request = new_request;
                        self.partial_response = Some(self.merge_and_strip(peer, response));
                        State::SendToNextPeer(NewRequestRoundReason::PartialData)
                    } else {
                        let response = self.merge_and_strip(peer, response);
                        return Poll::Ready(Ok(response));
                    }
                }
                State::AwaitingExternalValidation(peer, mut receiver, response) => {
                    match receiver.poll_unpin(ctx.task_cx) {
                        Poll::Ready(Ok(true)) => State::MergeAndStrip(peer, response),
                        Poll::Ready(Ok(false)) => {
                            State::SendToNextPeer(NewRequestRoundReason::PartialData)
                        }
                        Poll::Ready(Err(_recv_err)) => {
                            unreachable!("oneshot sender must never be dropped")
                        }
                        Poll::Pending => {
                            pending = true;
                            State::AwaitingExternalValidation(peer, receiver, response)
                        }
                    }
                }
            };
            self.state = Some(next_state);

            if pending {
                break Poll::Pending;
            }
        }
    }
}

#[derive(Debug, Clone)]
pub struct RetriableRequest {
    request_id: RequestId,

    // common state
    request: Request,
    partial_response: Option<Response>,
    tried_peers: HashSet<PeerId>,
    original_request: Request,
}

impl RetriableRequest {
    fn new(
        request_id: RequestId,
        OngoingRequest {
            state: _,
            timer: _,
            peer_score_handle: _,
            request,
            partial_response,
            original_request,
            tried_peers,
        }: OngoingRequest,
    ) -> Self {
        Self {
            request_id,
            request,
            partial_response,
            tried_peers,
            original_request,
        }
    }
}

impl PartialEq for RetriableRequest {
    fn eq(&self, other: &Self) -> bool {
        self.request_id == other.request_id
    }
}

impl Eq for RetriableRequest {}

impl RetriableRequest {
    pub fn id(&self) -> RequestId {
        self.request_id
    }
}
