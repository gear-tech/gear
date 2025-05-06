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
use tokio::sync::oneshot::{Receiver, Sender};

#[derive(Debug, Clone)]
pub struct RetriableRequest {
    request_id: RequestId,
    state: OngoingRequestState,
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

pub(crate) struct OngoingRequests {
    requests: HashMap<RequestId, Option<OngoingRequest>>,
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
        let ctx = OngoingRequestState {
            tried_peers: HashSet::new(),
            peer_score_handle: self.peer_score_handle.clone(),
            original_request: request,
            request_timeout: self.request_timeout,
            max_rounds_per_request: self.max_rounds_per_request,
        };
        self.requests.insert(request_id, Some(Initial::create(ctx)));
        request_id
    }

    pub(crate) fn retry(&mut self, request: RetriableRequest) {
        self.requests
            .insert(request.request_id, Some(Initial::create(request.state)));
    }

    fn update_request(
        &mut self,
        request_id: RequestId,
        f: impl FnOnce(OngoingRequest) -> OngoingRequest,
    ) {
        let request = self
            .requests
            .get_mut(&request_id)
            .expect("unknown request id");
        let next_state = f(request.take().expect("always Some"));
        *request = Some(next_state);
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
        self.update_request(request_id, |state| {
            state.on_peer_response(peer, response, false)
        })
    }

    pub(crate) fn on_peer_failure(&mut self, peer: PeerId, outbound_request_id: OutboundRequestId) {
        let request_id = self
            .active_requests
            .remove(&outbound_request_id)
            .expect("unknown outbound request id");
        self.update_request(request_id, |state| state.on_peer_failure(peer))
    }

    pub(crate) fn poll_next_states(
        &mut self,
        cx: &mut Context<'_>,
        behaviour: &mut InnerBehaviour,
    ) -> Vec<Event> {
        let mut events = Vec::new();
        let mut to_remove = Vec::new();

        for (&request_id, request) in &mut self.requests {
            let mut ctx = OngoingRequestContext {
                task_cx: cx,
                pending_events: VecDeque::new(),
                connections: &self.connections,
            };

            let next_state = request
                .take()
                .expect("always Some")
                .poll_next_state(&mut ctx);

            if let OngoingRequest::Succeed(_) | OngoingRequest::Failed(_) = &next_state {
                debug_assert!(ctx.pending_events.is_empty());

                let event = if let OngoingRequest::Succeed(Succeed { response }) = next_state {
                    Event::RequestSucceed {
                        request_id,
                        response,
                    }
                } else if let OngoingRequest::Failed(Failed { state, error }) = next_state {
                    Event::RequestFailed {
                        request: RetriableRequest { request_id, state },
                        error,
                    }
                } else {
                    unreachable!()
                };
                events.push(event);
                to_remove.push(request_id);

                continue;
            }

            *request = Some(next_state);

            for event in ctx.pending_events {
                match event {
                    OngoingRequestEvent::SendRequest(peer, request) => {
                        let outbound_request_id = behaviour.send_request(&peer, request);
                        self.active_requests.insert(outbound_request_id, request_id);
                    }
                    OngoingRequestEvent::ExternalValidationRequired(sender, response) => events
                        .push(Event::ExternalValidationRequired {
                            request_id,
                            response,
                            sender,
                        }),
                }
            }
        }

        for request_id in to_remove {
            self.requests.remove(&request_id);
        }

        events
    }
}

#[enum_delegate::register]
trait StateHandler
where
    Self: Sized,
    OngoingRequest: From<Self>,
{
    fn on_peer_response(
        self,
        _peer: PeerId,
        _response: Response,
        _already_verified: bool,
    ) -> OngoingRequest;

    fn on_peer_failure(self, _peer: PeerId) -> OngoingRequest;

    fn poll_next_state(self, _ctx: &mut OngoingRequestContext) -> OngoingRequest;
}

#[derive(Debug)]
enum OngoingRequestEvent {
    SendRequest(PeerId, Request),
    ExternalValidationRequired(Sender<bool>, Response),
}

struct OngoingRequestContext<'a, 'cx> {
    task_cx: &'a mut Context<'cx>,
    pending_events: VecDeque<OngoingRequestEvent>,
    connections: &'a ConnectionMap,
}

#[derive(Debug, Clone)]
struct OngoingRequestState {
    tried_peers: HashSet<PeerId>,
    peer_score_handle: Handle,
    original_request: Request,
    request_timeout: Duration,
    max_rounds_per_request: u32,
}

#[enum_delegate::implement(StateHandler)]
enum OngoingRequest {
    Initial(Initial),
    Active(Active),
    ExternalValidation(ExternalValidation),
    Failed(Failed),
    Succeed(Succeed),
}

struct Initial {
    state: OngoingRequestState,
}

impl Initial {
    fn create(state: OngoingRequestState) -> OngoingRequest {
        Self { state }.into()
    }
}

impl StateHandler for Initial {
    fn on_peer_response(
        self,
        _peer: PeerId,
        _response: Response,
        _already_verified: bool,
    ) -> OngoingRequest {
        unreachable!()
    }

    fn on_peer_failure(self, _peer: PeerId) -> OngoingRequest {
        unreachable!()
    }

    fn poll_next_state(self, _ctx: &mut OngoingRequestContext) -> OngoingRequest {
        Active::create(self.state)
    }
}

struct Active {
    state: OngoingRequestState,
    timer: Timer,
    request: Request,
    request_sent: bool,
    partial_response: Option<Response>,
}

impl Active {
    fn create(state: OngoingRequestState) -> OngoingRequest {
        let mut timer = Timer::new("ongoing-request", state.request_timeout);
        timer.start(());

        let request = state.original_request.clone();

        Self {
            state,
            timer,
            request,
            request_sent: false,
            partial_response: None,
        }
        .into()
    }

    fn merge_and_strip(&mut self, peer: PeerId, new_response: Response) -> Response {
        let mut response = if let Some(mut response) = self.partial_response.take() {
            response.merge(new_response);
            response
        } else {
            new_response
        };

        if response.strip(&self.state.original_request) {
            log::debug!("data stripped in response from {peer}");
            self.state.peer_score_handle.excessive_data(peer);
        }

        response
    }

    fn choose_next_peer(&mut self, connections: &ConnectionMap) -> Option<PeerId> {
        let peers: HashSet<PeerId> = connections.peers().collect();
        let peer = peers
            .difference(&self.state.tried_peers)
            .choose_stable(&mut rand::thread_rng())
            .copied();
        peer
    }
}

impl StateHandler for Active {
    fn on_peer_response(
        mut self,
        peer: PeerId,
        response: Response,
        already_verified: bool,
    ) -> OngoingRequest {
        self.state.tried_peers.insert(peer);
        self.request_sent = false;

        match response.validate() {
            Ok(is_valid) => {
                if is_valid || already_verified {
                    if let Some(new_request) = self.request.difference(&response) {
                        self.request = new_request;
                        self.partial_response = Some(self.merge_and_strip(peer, response));
                        self.into()
                    } else {
                        let response = self.merge_and_strip(peer, response);
                        Succeed::create(response)
                    }
                } else {
                    ExternalValidation::create(self, peer, response)
                }
            }
            Err(error) => {
                log::trace!("response validation failed for request from {peer}: {error:?}");
                self.state.peer_score_handle.invalid_data(peer);
                self.into()
            }
        }
    }

    fn on_peer_failure(mut self, peer: PeerId) -> OngoingRequest {
        self.state.tried_peers.insert(peer);
        self.request_sent = false;
        self.into()
    }

    fn poll_next_state(mut self, ctx: &mut OngoingRequestContext) -> OngoingRequest {
        if let Poll::Ready(()) = self.timer.poll_unpin(ctx.task_cx) {
            return Failed::create(self.state, RequestFailure::Timeout);
        }

        if self.state.tried_peers.len() > self.state.max_rounds_per_request as usize {
            return Failed::create(self.state, RequestFailure::OutOfRounds);
        }

        if !self.request_sent {
            self.request_sent = true;

            let Some(peer) = self.choose_next_peer(ctx.connections) else {
                return Initial::create(self.state);
            };
            ctx.pending_events
                .push_back(OngoingRequestEvent::SendRequest(peer, self.request.clone()));
        }

        self.into()
    }
}

struct ExternalValidation {
    active: Active,
    peer: PeerId,
    response: Response,
    sender: Option<Sender<bool>>,
    receiver: Receiver<bool>,
}

impl ExternalValidation {
    fn create(active: Active, peer: PeerId, response: Response) -> OngoingRequest {
        let (sender, receiver) = tokio::sync::oneshot::channel();
        Self {
            active,
            peer,
            response,
            sender: Some(sender),
            receiver,
        }
        .into()
    }
}

impl StateHandler for ExternalValidation {
    fn on_peer_response(
        self,
        _peer: PeerId,
        _response: Response,
        _already_verified: bool,
    ) -> OngoingRequest {
        unreachable!()
    }

    fn on_peer_failure(self, _peer: PeerId) -> OngoingRequest {
        unreachable!()
    }

    fn poll_next_state(mut self, ctx: &mut OngoingRequestContext) -> OngoingRequest {
        if let Some(sender) = self.sender.take() {
            ctx.pending_events
                .push_back(OngoingRequestEvent::ExternalValidationRequired(
                    sender,
                    self.response.clone(),
                ));
        }

        if let Poll::Ready(res) = self.receiver.poll_unpin(ctx.task_cx) {
            return match res {
                Ok(true) => self.active.on_peer_response(self.peer, self.response, true),
                Ok(false) => self.active.on_peer_failure(self.peer),
                Err(_recv_err) => {
                    unreachable!("oneshot sender must never be dropped")
                }
            };
        }

        self.into()
    }
}

struct Succeed {
    response: Response,
}

impl Succeed {
    fn create(response: Response) -> OngoingRequest {
        Self { response }.into()
    }
}

impl StateHandler for Succeed {
    fn on_peer_response(
        self,
        _peer: PeerId,
        _response: Response,
        _already_verified: bool,
    ) -> OngoingRequest {
        unreachable!()
    }

    fn on_peer_failure(self, _peer: PeerId) -> OngoingRequest {
        unreachable!()
    }

    fn poll_next_state(self, _ctx: &mut OngoingRequestContext) -> OngoingRequest {
        self.into()
    }
}

struct Failed {
    state: OngoingRequestState,
    error: RequestFailure,
}

impl Failed {
    fn create(state: OngoingRequestState, error: RequestFailure) -> OngoingRequest {
        Self { state, error }.into()
    }
}

impl StateHandler for Failed {
    fn on_peer_response(
        self,
        _peer: PeerId,
        _response: Response,
        _already_verified: bool,
    ) -> OngoingRequest {
        unreachable!()
    }

    fn on_peer_failure(self, _peer: PeerId) -> OngoingRequest {
        unreachable!()
    }

    fn poll_next_state(self, _ctx: &mut OngoingRequestContext) -> OngoingRequest {
        self.into()
    }
}
