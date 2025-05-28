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
        Config, Event, InnerBehaviour, InnerResponse, InnerResponseProcessor,
        NewRequestRoundReason, PeerId, Request, RequestFailure, RequestId, Response,
    },
    peer_score::Handle,
    utils::ConnectionMap,
};
use ethexe_ethereum::router::RouterQuery;
use futures::{future::BoxFuture, FutureExt};
use libp2p::{
    request_response::OutboundRequestId,
    swarm::{behaviour::ConnectionEstablished, ConnectionClosed, FromSwarm},
};
use rand::prelude::IteratorRandom;
use std::{
    cell::OnceCell,
    collections::{HashMap, HashSet, VecDeque},
    task::{Context, Poll, Waker},
    time::Duration,
};
use tokio::{sync::oneshot::Sender, time};

ethexe_service_utils::task_local! {
    static CONTEXT: OngoingRequestContext;
}

type OngoingRequestFuture = BoxFuture<'static, Result<Response, (RequestFailure, OngoingRequest)>>;

pub(crate) struct OngoingRequests {
    pending_events: VecDeque<Event>,
    requests: HashMap<RequestId, OngoingRequestFuture>,
    active_requests: HashMap<OutboundRequestId, RequestId>,
    responses: HashMap<RequestId, Result<InnerResponse, ()>>,
    connections: ConnectionMap,
    waker: Option<Waker>,
    request_id_counter: u64,
    //
    peer_score_handle: Handle,
    router_query: RouterQuery,
    // config
    request_timeout: Duration,
    max_rounds_per_request: u32,
}

impl OngoingRequests {
    pub(crate) fn new(
        config: &Config,
        peer_score_handle: Handle,
        router_query: RouterQuery,
    ) -> Self {
        Self {
            pending_events: VecDeque::new(),
            requests: Default::default(),
            active_requests: Default::default(),
            responses: Default::default(),
            connections: Default::default(),
            waker: None,
            request_id_counter: 0,
            peer_score_handle,
            router_query,
            request_timeout: config.request_timeout,
            max_rounds_per_request: config.max_rounds_per_request,
        }
    }

    fn wake(&mut self) {
        if let Some(waker) = self.waker.take() {
            waker.wake();
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
                self.wake();
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
                    self.router_query.clone(),
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
                    self.router_query.clone(),
                    self.request_timeout,
                    self.max_rounds_per_request,
                )
                .boxed(),
        );
    }

    fn inner_on_peer(
        &mut self,
        outbound_request_id: OutboundRequestId,
        res: Result<InnerResponse, ()>,
    ) {
        let request_id = self
            .active_requests
            .remove(&outbound_request_id)
            .expect("unknown outbound request id");
        let fut = self.requests.get_mut(&request_id);

        // request can be removed because of timeout,
        // so we don't expect it's still inside `self.requests`
        if fut.is_some() {
            self.responses.insert(request_id, res);
            self.wake();
        } else {
            log::trace!("{outbound_request_id:?} has been skipped for {request_id:?}");
        }
    }

    pub(crate) fn on_peer_response(
        &mut self,
        outbound_request_id: OutboundRequestId,
        response: InnerResponse,
    ) {
        self.inner_on_peer(outbound_request_id, Ok(response));
    }

    pub(crate) fn on_peer_failure(&mut self, outbound_request_id: OutboundRequestId) {
        self.inner_on_peer(outbound_request_id, Err(()));
    }

    pub(crate) fn poll(
        &mut self,
        cx: &mut Context<'_>,
        behaviour: &mut InnerBehaviour,
    ) -> Poll<Event> {
        loop {
            if let Some(event) = self.pending_events.pop_front() {
                return Poll::Ready(event);
            }

            let peers: HashSet<PeerId> = self.connections.peers().collect();

            self.requests.retain(|&request_id, fut| {
                let response = self.responses.remove(&request_id);
                let ctx = OngoingRequestContext {
                    state: OnceCell::new(),
                    peers: peers.clone(),
                    response,
                };

                let (ctx, poll) = CONTEXT.scope(ctx, || fut.poll_unpin(cx));
                let state = ctx.into_state();
                if state.is_some() && poll.is_ready() {
                    unreachable!(
                        "state machine invariant violated: unexpected ready poll with existing state"
                    );
                }

                if let Some(state) = state {
                    let event = match state {
                        OngoingRequestState::PendingState => Event::PendingStateRequest { request_id },
                        OngoingRequestState::SendRequest(peer, request, reason) => {
                            let outbound_request_id = behaviour.send_request(&peer, request);
                            self.active_requests.insert(outbound_request_id, request_id);

                            Event::NewRequestRound {
                                request_id,
                                peer_id: peer,
                                reason,
                            }
                        }
                        OngoingRequestState::ExternalValidationRequired(sender, response) => {
                            Event::ExternalValidationRequired {
                                request_id,
                                response,
                                sender,
                            }
                        }
                    };
                    self.pending_events.push_back(event);
                } else if let Poll::Ready(res) = poll {
                    let event = match res {
                        Ok(response) => Event::RequestSucceed {
                            request_id,
                            response,
                        },
                        Err((error, request)) => Event::RequestFailed {
                            request: RetriableRequest {
                                request_id,
                                request,
                            },
                            error,
                        }
                    };
                    self.pending_events.push_back(event);
                    return false;
                }

                true
            });

            // it means some futures are pending, so we definitely will wake the task
            if !self.requests.is_empty() {
                self.waker = Some(cx.waker().clone());
            }

            if !self.pending_events.is_empty() {
                // immediately return event instead of task waking
                continue;
            }

            break Poll::Pending;
        }
    }
}

impl Drop for OngoingRequests {
    fn drop(&mut self) {
        #[cfg(test)]
        {
            assert!(self.pending_events.is_empty());
            assert!(self.requests.is_empty());
            assert!(self.active_requests.is_empty());
            assert!(self.requests.is_empty());
        }
    }
}

#[derive(Debug)]
struct OngoingRequest {
    response_processor: InnerResponseProcessor,
    tried_peers: HashSet<PeerId>,
}

impl OngoingRequest {
    fn new(request: Request) -> Self {
        Self {
            response_processor: InnerResponseProcessor::new(request),
            tried_peers: Default::default(),
        }
    }

    async fn choose_next_peer(&mut self) -> (PeerId, Option<NewRequestRoundReason>) {
        let mut event_sent = None;

        let peer = CONTEXT
            .poll_fn(|_task_cx, ctx| {
                let peer = ctx
                    .peers
                    .difference(&self.tried_peers)
                    .choose_stable(&mut rand::thread_rng())
                    .copied();
                self.tried_peers.extend(peer);

                if let Some(peer) = peer {
                    Poll::Ready(peer)
                } else {
                    event_sent.get_or_insert_with(|| {
                        ctx.state
                            .set(OngoingRequestState::PendingState)
                            .expect("set only once");
                    });

                    Poll::Pending
                }
            })
            .await;

        let reason = event_sent.map(|()| NewRequestRoundReason::FromQueue);
        (peer, reason)
    }

    async fn send_request(
        &mut self,
        peer: PeerId,
        reason: NewRequestRoundReason,
    ) -> Result<InnerResponse, ()> {
        CONTEXT.with_mut(|ctx| {
            ctx.state
                .set(OngoingRequestState::SendRequest(
                    peer,
                    self.response_processor.request(),
                    reason,
                ))
                .expect("set only once");
        });

        CONTEXT
            .poll_fn(|_task_cx, ctx| {
                if let Some(res) = ctx.response.take() {
                    Poll::Ready(res)
                } else {
                    Poll::Pending
                }
            })
            .await
    }

    async fn next_round(
        &mut self,
        mut reason: NewRequestRoundReason,
        peer_score_handle: &Handle,
        router_query: &RouterQuery,
    ) -> Result<Response, NewRequestRoundReason> {
        let (peer, new_reason) = self.choose_next_peer().await;
        reason = new_reason.unwrap_or(reason);

        let response = self
            .send_request(peer, reason)
            .await
            .map_err(|()| NewRequestRoundReason::PeerFailed)?;

        match self
            .response_processor
            .process(peer, response, router_query, peer_score_handle)
            .await
        {
            Ok(response) => Ok(response),
            Err(err) => {
                log::trace!("response processing failed for request from {peer}: {err:?}");
                peer_score_handle.invalid_data(peer);
                Err(NewRequestRoundReason::PartialData)
            }
        }
    }

    async fn request(
        mut self,
        peer_score_handle: Handle,
        router_query: RouterQuery,
        request_timeout: Duration,
        max_rounds_per_request: u32,
    ) -> Result<Response, (RequestFailure, Self)> {
        let request_loop = async {
            let mut rounds = 0;
            let mut reason = NewRequestRoundReason::FromQueue;

            loop {
                if rounds >= max_rounds_per_request {
                    return Err(RequestFailure::OutOfRounds);
                }
                rounds += 1;

                match self
                    .next_round(reason, &peer_score_handle, &router_query)
                    .await
                {
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
enum OngoingRequestState {
    PendingState,
    SendRequest(PeerId, Request, NewRequestRoundReason),
    ExternalValidationRequired(Sender<bool>, Response),
}

struct OngoingRequestContext {
    state: OnceCell<OngoingRequestState>,
    peers: HashSet<PeerId>,
    response: Option<Result<InnerResponse, ()>>,
}

impl OngoingRequestContext {
    fn into_state(self) -> Option<OngoingRequestState> {
        let Self {
            state,
            peers: _,
            response,
        } = self;
        let state = state.into_inner();
        debug_assert_eq!(response, None, "future must take provided response");
        state
    }
}

#[derive(Debug)]
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
