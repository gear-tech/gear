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
use futures::{future, future::BoxFuture, FutureExt};
use libp2p::{
    request_response::OutboundRequestId,
    swarm::{behaviour::ConnectionEstablished, ConnectionClosed, FromSwarm},
};
use rand::prelude::IteratorRandom;
use std::{
    cell::RefCell,
    collections::{HashMap, HashSet, VecDeque},
    task::{Context, Poll, Waker},
    time::Duration,
};
use tokio::{
    sync::{oneshot, oneshot::Sender},
    time,
};

type OngoingRequestFuture = BoxFuture<'static, Result<Response, (RequestFailure, OngoingRequest)>>;

pub(crate) struct OngoingRequests {
    requests: HashMap<RequestId, OngoingRequestFuture>,
    active_requests: HashMap<OutboundRequestId, RequestId>,
    responses: HashMap<RequestId, Result<Response, ()>>,
    connections: ConnectionMap,
    waker: Option<Waker>,
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
            responses: Default::default(),
            connections: Default::default(),
            waker: None,
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

                if let Some(waker) = self.waker.take() {
                    waker.wake();
                }
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
        self.requests.insert(
            request_id,
            OngoingRequest::new(request)
                .request(
                    self.peer_score_handle.clone(),
                    self.request_timeout,
                    self.max_rounds_per_request,
                )
                .boxed(),
        );
        request_id
    }

    pub(crate) fn retry(&mut self, request: RetriableRequest) {
        let RetriableRequest {
            request_id,
            request,
        } = request;
        self.requests.insert(
            request_id,
            request
                .request(
                    self.peer_score_handle.clone(),
                    self.request_timeout,
                    self.max_rounds_per_request,
                )
                .boxed(),
        );
    }

    pub(crate) fn on_peer_response(
        &mut self,
        outbound_request_id: OutboundRequestId,
        response: Response,
    ) {
        let request_id = self
            .active_requests
            .remove(&outbound_request_id)
            .expect("unknown outbound request id");
        let request = self.requests.get_mut(&request_id);
        if let Some(_request) = request {
            self.responses.insert(request_id, Ok(response));
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
        if let Some(_request) = request {
            self.responses.insert(request_id, Err(()));
        } else {
            log::trace!("request {outbound_request_id} has been skipped");
        }
    }

    pub(crate) fn poll(
        &mut self,
        cx: &mut Context<'_>,
        behaviour: &mut InnerBehaviour,
    ) -> Poll<Vec<Event>> {
        let mut events = Vec::new();
        let mut kept = Vec::new();

        for (request_id, mut fut) in self.requests.drain() {
            let response = self.responses.remove(&request_id);
            let ctx = OngoingRequestContext {
                pending_events: VecDeque::new(),
                peers: self.connections.peers().collect(),
                response,
            };

            CONTEXT.set(Some(ctx));
            let poll = fut.poll_unpin(cx);
            let ctx = CONTEXT.take().expect("context must be set");

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

            log::error!("poll: {poll:?}");
            match poll {
                Poll::Ready(Ok(response)) => events.push(Event::RequestSucceed {
                    request_id,
                    response,
                }),
                Poll::Ready(Err((error, request))) => events.push(Event::RequestFailed {
                    request: RetriableRequest {
                        request_id,
                        request,
                    },
                    error,
                }),
                Poll::Pending => {
                    kept.push((request_id, fut));
                }
            }
        }

        if !kept.is_empty() {
            self.waker = Some(cx.waker().clone());
            self.requests.extend(kept);
        }

        if events.is_empty() {
            Poll::Pending
        } else {
            Poll::Ready(events)
        }
    }
}

#[derive(Debug, Clone)]
struct OngoingRequest {
    request: Request,
    partial_response: Option<Response>,
    original_request: Request,
    tried_peers: HashSet<PeerId>,
}

impl OngoingRequest {
    fn new(request: Request) -> Self {
        Self {
            request: request.clone(),
            partial_response: None,
            original_request: request,
            tried_peers: Default::default(),
        }
    }

    async fn choose_next_peer(&mut self) -> (PeerId, Option<NewRequestRoundReason>) {
        let mut event_sent = false;

        let peer = poll_context(|ctx| {
            log::debug!("connections: {:?}", ctx.peers);
            let peer = ctx
                .peers
                .difference(&self.tried_peers)
                .choose_stable(&mut rand::thread_rng())
                .copied();
            self.tried_peers.extend(peer);

            if let Some(peer) = peer {
                Poll::Ready(peer)
            } else {
                if !event_sent {
                    ctx.pending_events
                        .push_back(OngoingRequestEvent::PendingState);
                    event_sent = true;
                }

                Poll::Pending
            }
        })
        .await;

        let event = Some(NewRequestRoundReason::FromQueue).filter(|_| event_sent);
        (peer, event)
    }

    async fn send_request(
        &mut self,
        peer: PeerId,
        reason: NewRequestRoundReason,
    ) -> Result<Response, ()> {
        context(|ctx| {
            ctx.pending_events
                .push_back(OngoingRequestEvent::SendRequest(
                    peer,
                    self.request.clone(),
                    reason,
                ));
        })
        .await;

        poll_context(|ctx| {
            if let Some(res) = ctx.response.take() {
                Poll::Ready(res)
            } else {
                Poll::Pending
            }
        })
        .await
    }

    fn merge_and_strip(
        &mut self,
        peer_score_handle: &Handle,
        peer: PeerId,
        new_response: Response,
    ) -> Response {
        let mut response = if let Some(mut response) = self.partial_response.take() {
            response.merge(new_response);
            response
        } else {
            new_response
        };

        if response.strip(&self.original_request) {
            log::debug!("data stripped in response from {peer}");
            peer_score_handle.excessive_data(peer);
        }

        response
    }

    async fn next_round(
        &mut self,
        mut reason: NewRequestRoundReason,
        peer_score_handle: &Handle,
    ) -> Result<Response, NewRequestRoundReason> {
        let (peer, event) = self.choose_next_peer().await;
        reason = event.unwrap_or(reason);

        let response = self
            .send_request(peer, reason)
            .await
            .map_err(|()| NewRequestRoundReason::PeerFailed)?;

        let no_external_validation = match response.validate() {
            Ok(is_valid) => is_valid,
            Err(err) => {
                log::trace!("response validation failed for request from {peer}: {err:?}");
                peer_score_handle.invalid_data(peer);
                return Err(NewRequestRoundReason::PartialData);
            }
        };
        if !no_external_validation {
            let (sender, receiver) = oneshot::channel();
            context(|ctx| {
                ctx.pending_events
                    .push_back(OngoingRequestEvent::ExternalValidationRequired(
                        sender,
                        response.clone(),
                    ));
            })
            .await;
            let is_valid = receiver
                .await
                .expect("oneshot receiver must never be dropped");
            if !is_valid {
                return Err(NewRequestRoundReason::PartialData);
            }
        }

        let response = self.merge_and_strip(peer_score_handle, peer, response);

        if let Some(new_request) = self.original_request.difference(&response) {
            self.request = new_request;
            self.partial_response = Some(response);
            return Err(NewRequestRoundReason::PartialData);
        }

        Ok(response)
    }

    async fn request(
        mut self,
        peer_score_handle: Handle,
        request_timeout: Duration,
        max_rounds_per_request: u32,
    ) -> Result<Response, (RequestFailure, Self)> {
        let request_loop = async {
            let mut rounds = 0;
            let mut reason = NewRequestRoundReason::FromQueue;

            loop {
                log::error!("REASON: {reason:?}");

                if rounds >= max_rounds_per_request {
                    return Err(RequestFailure::OutOfRounds);
                }
                rounds += 1;

                match self.next_round(reason, &peer_score_handle).await {
                    Ok(response) => return Ok(response),
                    Err(new_reason) => {
                        reason = new_reason;
                    }
                };
            }
        };

        let res = time::timeout(request_timeout, request_loop)
            .await
            .map_err(|_elapsed| RequestFailure::Timeout)
            .and_then(|res| res);
        res.map_err(|failure| (failure, self))
    }
}

#[derive(Debug)]
enum OngoingRequestEvent {
    PendingState,
    SendRequest(PeerId, Request, NewRequestRoundReason),
    ExternalValidationRequired(Sender<bool>, Response),
}

struct OngoingRequestContext {
    pending_events: VecDeque<OngoingRequestEvent>,
    peers: HashSet<PeerId>,
    response: Option<Result<Response, ()>>,
}

thread_local! {
    static CONTEXT: RefCell<Option<OngoingRequestContext>> = const { RefCell::new(None) };
}

fn context<T>(f: impl FnOnce(&mut OngoingRequestContext) -> T) -> impl Future<Output = T> {
    future::lazy(|_cx| {
        CONTEXT.with_borrow_mut(|ctx| {
            let ctx = ctx.as_mut().expect("context must be set");
            f(ctx)
        })
    })
}

fn poll_context<T>(
    mut f: impl FnMut(&mut OngoingRequestContext) -> Poll<T>,
) -> impl Future<Output = T> {
    future::poll_fn(move |_cx| {
        CONTEXT.with_borrow_mut(|ctx| {
            let ctx = ctx.as_mut().expect("context must be set");
            f(ctx)
        })
    })
}

#[derive(Debug, Clone)]
pub struct RetriableRequest {
    request_id: RequestId,
    request: OngoingRequest,
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
