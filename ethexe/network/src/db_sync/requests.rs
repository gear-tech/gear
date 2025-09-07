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
        AnnouncesRequest, Config, Event, ExternalDataProvider, HandleResult, HashesRequest,
        InnerAnnouncesRequest, InnerBehaviour, InnerHashesResponse, InnerProgramIdsRequest,
        InnerProgramIdsResponse, InnerRequest, InnerResponse, NewRequestRoundReason, PeerId,
        ProgramIdsRequest, Request, RequestFailure, RequestId, Response, ValidCodesRequest,
    },
    peer_score::Handle,
    utils::ConnectionMap,
};
use anyhow::Context as _;
use ethexe_common::{Announce, AnnounceHash, gear::CodeState};
use futures::{FutureExt, future::BoxFuture};
use gprimitives::{ActorId, CodeId, H256};
use itertools::EitherOrBoth;
use libp2p::{
    request_response::OutboundRequestId,
    swarm::{ConnectionClosed, FromSwarm, behaviour::ConnectionEstablished},
};
use rand::prelude::IteratorRandom;
use std::{
    cell::OnceCell,
    collections::{BTreeMap, BTreeSet, HashMap, HashSet, VecDeque},
    iter,
    task::{Context, Poll, Waker},
    time::Duration,
};
use tokio::{sync::oneshot, time};

ethexe_service_utils::task_local! {
    static CONTEXT: OngoingRequestContext;
}

type OngoingRequestFuture = BoxFuture<'static, Result<Response, (RequestFailure, OngoingRequest)>>;

pub(crate) struct OngoingRequests {
    pending_events: VecDeque<Event>,
    requests: HashMap<RequestId, (OngoingRequestFuture, Option<oneshot::Sender<HandleResult>>)>,
    active_requests: HashMap<OutboundRequestId, RequestId>,
    responses: HashMap<RequestId, Result<InnerResponse, ()>>,
    connections: ConnectionMap,
    waker: Option<Waker>,
    // used in requests themselves
    peer_score_handle: Handle,
    external_data_provider: Box<dyn ExternalDataProvider>,
    // config
    request_timeout: Duration,
    max_rounds_per_request: u32,
}

impl OngoingRequests {
    pub(crate) fn new(
        config: &Config,
        peer_score_handle: Handle,
        external_data_provider: Box<dyn ExternalDataProvider>,
    ) -> Self {
        Self {
            pending_events: VecDeque::new(),
            requests: Default::default(),
            active_requests: Default::default(),
            responses: Default::default(),
            connections: Default::default(),
            waker: None,
            peer_score_handle,
            external_data_provider,
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

    fn inner_request(
        &mut self,
        request_id: RequestId,
        request: OngoingRequest,
        channel: oneshot::Sender<HandleResult>,
    ) {
        self.requests.insert(
            request_id,
            (
                request
                    .request(
                        self.peer_score_handle.clone(),
                        self.external_data_provider.clone_boxed(),
                        self.request_timeout,
                        self.max_rounds_per_request,
                    )
                    .boxed(),
                Some(channel),
            ),
        );
    }

    pub(crate) fn request(
        &mut self,
        request_id: RequestId,
        request: Request,
        channel: oneshot::Sender<HandleResult>,
    ) {
        self.inner_request(request_id, OngoingRequest::new(request), channel);
    }

    pub(crate) fn retry(
        &mut self,
        request: RetriableRequest,
        channel: oneshot::Sender<HandleResult>,
    ) {
        let RetriableRequest {
            request_id,
            request,
        } = request;
        self.inner_request(request_id, request, channel);
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

            self.requests.retain(|&request_id, (fut, channel)| {
                let response = self.responses.remove(&request_id);

                if channel.as_ref().expect("always Some").is_closed() {
                    return false;
                }

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
                    };
                    self.pending_events.push_back(event);
                } else if let Poll::Ready(res) = poll {
                    let (event, res) = match res {
                        Ok(response) => {
                            (Event::RequestSucceed { request_id }, Ok(response))
                        }
                        Err((error, request)) => {
                            (Event::RequestFailed { request_id, error }, Err((error, RetriableRequest {
                                request_id,
                                request,
                            },
                            )))
                        }
                    };
                    self.pending_events.push_back(event);
                    let _res = channel.take().expect("always Some").send(res);
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

#[derive(Debug)]
enum HashesResponseHandled {
    Done {
        response: BTreeMap<H256, Vec<u8>>,
        stripped: bool,
    },
    NewRequest {
        acc: InnerHashesResponse,
        new_request: HashesRequest,
        stripped: bool,
    },
    Err {
        acc: InnerHashesResponse,
        err: HashesResponseError,
        stripped: bool,
    },
}

impl HashesResponseHandled {
    fn stripped(&self) -> bool {
        match self {
            Self::Done { stripped, .. } => *stripped,
            Self::NewRequest { stripped, .. } => *stripped,
            Self::Err { stripped, .. } => *stripped,
        }
    }
}

#[derive(Debug, Copy, Clone, Eq, PartialEq, derive_more::Display)]
pub enum HashesResponseError {
    #[display("hash mismatch from provided data")]
    HashMismatch,
}

#[derive(Debug, derive_more::Display)]
pub enum ProgramIdsResponseError {
    #[display("not enough program-code ids")]
    NotEnoughIds,
    #[display("router failed: {_0}")]
    RouterQuery(anyhow::Error),
}

#[derive(Debug, derive_more::Display)]
pub enum ValidCodesResponseError {
    #[display("not enough validated codes")]
    NotEnoughCodes,
    #[display("{_0}")]
    RouterQuery(anyhow::Error),
}

#[derive(Debug, derive_more::Display)]
pub enum AnnouncesResponseError {
    #[display("announces head mismatch, expected hash {expected}, received {received}")]
    HeadMismatch {
        expected: AnnounceHash,
        received: AnnounceHash,
    },
    #[display("announces len maximum {expected}, received {received}")]
    LenOverflow { expected: usize, received: usize },
    #[display("response is empty")]
    Empty,
}

#[derive(Debug, derive_more::Display, derive_more::From)]
enum ResponseError {
    #[display("{_0}")]
    Hashes(HashesResponseError),
    #[display("{_0}")]
    ProgramIds(ProgramIdsResponseError),
    #[display("{_0}")]
    ValidCodes(ValidCodesResponseError),
    #[display("{_0}")]
    Announces(AnnouncesResponseError),
    #[display("request and response types mismatch")]
    TypeMismatch,
    #[display("new round required")]
    NewRound,
}

#[derive(Debug)]
enum ResponseHandler {
    Hashes {
        acc: InnerHashesResponse,
        request: HashesRequest,
    },
    ProgramIds {
        request: ProgramIdsRequest,
    },
    ValidCodes {
        request: ValidCodesRequest,
    },
    Announces {
        request: AnnouncesRequest,
    },
}

impl ResponseHandler {
    fn new(request: Request) -> Self {
        match request {
            Request::Hashes(request) => Self::Hashes {
                acc: Default::default(),
                request,
            },
            Request::ProgramIds(request) => Self::ProgramIds { request },
            Request::ValidCodes(request) => Self::ValidCodes { request },
            Request::Announces(request) => Self::Announces { request },
        }
    }

    fn inner_request(&self) -> InnerRequest {
        match self {
            ResponseHandler::Hashes {
                request: reduced_request,
                ..
            } => InnerRequest::Hashes(reduced_request.clone()),
            ResponseHandler::ProgramIds {
                request:
                    ProgramIdsRequest {
                        at,
                        expected_count: _,
                    },
            } => InnerRequest::ProgramIds(InnerProgramIdsRequest { at: *at }),
            ResponseHandler::ValidCodes {
                request:
                    ValidCodesRequest {
                        at: _,
                        validated_count: _,
                    },
            } => InnerRequest::ValidCodes,
            ResponseHandler::Announces { request } => {
                InnerRequest::Announces(InnerAnnouncesRequest {
                    head: request.head,
                    max_chain_len: request.max_chain_len,
                })
            }
        }
    }

    fn handle_hashes(
        mut acc: InnerHashesResponse,
        reduced_request: &HashesRequest,
        new_response: InnerHashesResponse,
    ) -> HashesResponseHandled {
        let mut new_request = BTreeSet::new();
        let mut stripped = false;

        let diff = itertools::merge_join_by(
            reduced_request.0.iter().copied(),
            new_response.0,
            |req_key, (resp_key, _resp_val)| req_key.cmp(resp_key),
        );

        for either in diff {
            match either {
                EitherOrBoth::Both(req_key, (resp_key, resp_val)) => {
                    debug_assert_eq!(req_key, resp_key);
                    if req_key != ethexe_db::hash(&resp_val) {
                        return HashesResponseHandled::Err {
                            acc,
                            err: HashesResponseError::HashMismatch,
                            stripped,
                        };
                    }

                    acc.0.insert(resp_key, resp_val);
                }
                EitherOrBoth::Left(key) => {
                    // peer was unable to give this key
                    new_request.insert(key);
                }
                EitherOrBoth::Right(_key) => {
                    // peer sent more keys than we requested
                    stripped = true;
                }
            }
        }

        if new_request.is_empty() {
            HashesResponseHandled::Done {
                response: acc.0,
                stripped,
            }
        } else {
            HashesResponseHandled::NewRequest {
                acc,
                new_request: HashesRequest(new_request),
                stripped,
            }
        }
    }

    async fn handle_program_ids(
        response: InnerProgramIdsResponse,
        request: &ProgramIdsRequest,
        external_data_provider: Box<dyn ExternalDataProvider>,
    ) -> Result<BTreeMap<ActorId, CodeId>, ProgramIdsResponseError> {
        let InnerProgramIdsResponse(response) = response;

        if response.len() as u64 != request.expected_count {
            return Err(ProgramIdsResponseError::NotEnoughIds);
        }

        let code_ids = external_data_provider
            .programs_code_ids_at(response.clone(), request.at)
            .await
            .context("failed to get code ids at block")
            .map_err(ProgramIdsResponseError::RouterQuery)?;

        let program_code_ids = iter::zip(response, code_ids).collect();
        Ok(program_code_ids)
    }

    async fn handle_valid_codes(
        response: BTreeSet<CodeId>,
        request: &ValidCodesRequest,
        external_data_provider: Box<dyn ExternalDataProvider>,
    ) -> Result<BTreeSet<CodeId>, ValidCodesResponseError> {
        // validated count at specified block can be less than
        // the number of states at the latest block returned by peer
        // but cannot be more
        if (response.len() as u64) < request.validated_count {
            return Err(ValidCodesResponseError::NotEnoughCodes);
        }

        let states = external_data_provider
            .codes_states_at(response.clone(), request.at)
            .await
            .context("failed to get code states at block")
            .map_err(ValidCodesResponseError::RouterQuery)?;

        let code_ids: BTreeSet<CodeId> = iter::zip(response, states)
            .flat_map(|(code_id, state)| {
                if state == CodeState::Validated {
                    Some(code_id)
                } else {
                    None
                }
            })
            .collect();
        if request.validated_count != code_ids.len() as u64 {
            return Err(ValidCodesResponseError::NotEnoughCodes);
        }

        Ok(code_ids)
    }

    fn handle_announces(
        request: &AnnouncesRequest,
        response: Vec<Announce>,
    ) -> Result<Response, AnnouncesResponseError> {
        let Some(head) = response.first() else {
            return Err(AnnouncesResponseError::Empty);
        };

        if request.head != head.to_hash() {
            return Err(AnnouncesResponseError::HeadMismatch {
                expected: request.head,
                received: head.to_hash(),
            });
        }

        if response.len() > request.max_chain_len as usize {
            return Err(AnnouncesResponseError::LenOverflow {
                expected: request.max_chain_len as usize,
                received: response.len(),
            });
        }

        Ok(Response::Announces(response))
    }

    async fn handle(
        self,
        peer: PeerId,
        response: InnerResponse,
        peer_score_handle: &Handle,
        external_data_provider: Box<dyn ExternalDataProvider>,
    ) -> Result<Response, (Self, ResponseError)> {
        match (self, response) {
            (
                Self::Hashes {
                    acc,
                    request: reduced_request,
                },
                InnerResponse::Hashes(response),
            ) => {
                let processed = Self::handle_hashes(acc, &reduced_request, response);

                if processed.stripped() {
                    log::debug!("data stripped in response from {peer}");
                    peer_score_handle.excessive_data(peer);
                }

                match processed {
                    HashesResponseHandled::Done {
                        response,
                        stripped: _,
                    } => Ok(Response::Hashes(response)),
                    HashesResponseHandled::NewRequest {
                        acc,
                        new_request,
                        stripped: _,
                    } => Err((
                        Self::Hashes {
                            acc,
                            request: new_request,
                        },
                        ResponseError::NewRound,
                    )),
                    HashesResponseHandled::Err {
                        acc,
                        err,
                        stripped: _,
                    } => Err((
                        Self::Hashes {
                            acc,
                            request: reduced_request,
                        },
                        err.into(),
                    )),
                }
            }
            (Self::ProgramIds { request }, InnerResponse::ProgramIds(response)) => {
                Self::handle_program_ids(response, &request, external_data_provider)
                    .await
                    .map(Into::into)
                    .map_err(|err| (Self::ProgramIds { request }, err.into()))
            }
            (Self::ValidCodes { request }, InnerResponse::ValidCodes(response)) => {
                Self::handle_valid_codes(response, &request, external_data_provider)
                    .await
                    .map(Into::into)
                    .map_err(|err| (Self::ValidCodes { request }, err.into()))
            }
            (Self::Announces { request }, InnerResponse::Announces(response)) => {
                Self::handle_announces(&request, response)
                    .map_err(|err| (Self::Announces { request }, err.into()))
            }
            (this, _) => Err((this, ResponseError::TypeMismatch)),
        }
    }
}

#[derive(Debug)]
struct OngoingRequest {
    response_handler: Option<ResponseHandler>,
    tried_peers: HashSet<PeerId>,
}

impl OngoingRequest {
    fn new(request: Request) -> Self {
        Self {
            response_handler: Some(ResponseHandler::new(request)),
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
                    self.response_handler
                        .as_ref()
                        .expect("always Some")
                        .inner_request(),
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
        external_data_provider: Box<dyn ExternalDataProvider>,
    ) -> Result<Response, NewRequestRoundReason> {
        let (peer, new_reason) = self.choose_next_peer().await;
        reason = new_reason.unwrap_or(reason);

        let response = self
            .send_request(peer, reason)
            .await
            .map_err(|()| NewRequestRoundReason::PeerFailed)?;

        match self
            .response_handler
            .take()
            .expect("always Some")
            .handle(peer, response, peer_score_handle, external_data_provider)
            .await
        {
            Ok(response) => Ok(response),
            Err((processor, err)) => {
                log::trace!("response processing failed for request from {peer}: {err:?}");
                peer_score_handle.invalid_data(peer);
                self.response_handler = Some(processor);
                Err(NewRequestRoundReason::PartialData)
            }
        }
    }

    async fn request(
        mut self,
        peer_score_handle: Handle,
        external_data_provider: Box<dyn ExternalDataProvider>,
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
                    .next_round(
                        reason,
                        &peer_score_handle,
                        external_data_provider.clone_boxed(),
                    )
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
    SendRequest(PeerId, InnerRequest, NewRequestRoundReason),
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn validate_data_stripped() {
        let hash1 = ethexe_db::hash(b"1");
        let hash2 = ethexe_db::hash(b"2");
        let hash3 = ethexe_db::hash(b"3");

        let request = HashesRequest([hash1, hash2].into());
        let response = InnerHashesResponse(
            [
                (hash1, b"1".to_vec()),
                (hash2, b"2".to_vec()),
                (hash3, b"3".to_vec()),
            ]
            .into(),
        );
        let processed = ResponseHandler::handle_hashes(Default::default(), &request, response);
        let HashesResponseHandled::Done { response, stripped } = processed else {
            unreachable!("{processed:?}")
        };
        assert_eq!(
            response,
            BTreeMap::from_iter([(hash1, b"1".to_vec()), (hash2, b"2".to_vec())])
        );
        assert!(stripped);
    }

    #[test]
    fn validate_data_hash_mismatch() {
        let hash1 = ethexe_db::hash(b"1");

        let request = HashesRequest([hash1].into());
        let response = InnerHashesResponse([(hash1, b"2".to_vec())].into());
        let processed = ResponseHandler::handle_hashes(Default::default(), &request, response);
        let HashesResponseHandled::Err { acc, err, stripped } = processed else {
            unreachable!("{processed:?}")
        };
        assert_eq!(acc, Default::default());
        assert_eq!(err, HashesResponseError::HashMismatch);
        assert!(!stripped);
    }
}
