// Copyright (C) Parity Technologies (UK) Ltd.
// SPDX-License-Identifier: GPL-3.0-or-later WITH Classpath-exception-2.0

//! Generating request logic for request/response protocol for syncing BEEFY justifications.

use codec::Encode;
use futures::channel::{oneshot, oneshot::Canceled};
use log::{debug, warn};
use parking_lot::Mutex;
use sc_network::{
    request_responses::{IfDisconnected, RequestFailure},
    NetworkRequest, ProtocolName,
};
use sc_network_types::PeerId;
use sp_consensus_beefy::{AuthorityIdBound, ValidatorSet};
use sp_runtime::traits::{Block, NumberFor};
use std::{collections::VecDeque, result::Result, sync::Arc};

use crate::{
    communication::{
        benefit, cost,
        peers::PeerReport,
        request_response::{Error, JustificationRequest, BEEFY_SYNC_LOG_TARGET},
    },
    justification::{decode_and_verify_finality_proof, BeefyVersionedFinalityProof},
    metric_inc,
    metrics::{register_metrics, OnDemandOutgoingRequestsMetrics},
    KnownPeers,
};

/// Response type received from network.
type Response = Result<(Vec<u8>, ProtocolName), RequestFailure>;
/// Used to receive a response from the network.
type ResponseReceiver = oneshot::Receiver<Response>;

#[derive(Clone, Debug)]
struct RequestInfo<B: Block, AuthorityId: AuthorityIdBound> {
    block: NumberFor<B>,
    active_set: ValidatorSet<AuthorityId>,
}

enum State<B: Block, AuthorityId: AuthorityIdBound> {
    Idle,
    AwaitingResponse(PeerId, RequestInfo<B, AuthorityId>, ResponseReceiver),
    WaitingForPeers(RequestInfo<B, AuthorityId>),
}

/// Possible engine responses.
pub(crate) enum ResponseInfo<B: Block, AuthorityId: AuthorityIdBound> {
    /// No peer response available yet.
    Pending,
    /// Valid justification provided alongside peer reputation changes.
    ValidProof(BeefyVersionedFinalityProof<B, AuthorityId>, PeerReport),
    /// No justification yet, only peer reputation changes.
    PeerReport(PeerReport),
}

pub struct OnDemandJustificationsEngine<B: Block, AuthorityId: AuthorityIdBound> {
    network: Arc<dyn NetworkRequest + Send + Sync>,
    protocol_name: ProtocolName,

    live_peers: Arc<Mutex<KnownPeers<B>>>,
    peers_cache: VecDeque<PeerId>,
    peers_revision: u64,

    state: State<B, AuthorityId>,
    metrics: Option<OnDemandOutgoingRequestsMetrics>,
}

impl<B: Block, AuthorityId: AuthorityIdBound> OnDemandJustificationsEngine<B, AuthorityId> {
    pub fn new(
        network: Arc<dyn NetworkRequest + Send + Sync>,
        protocol_name: ProtocolName,
        live_peers: Arc<Mutex<KnownPeers<B>>>,
        prometheus_registry: Option<prometheus_endpoint::Registry>,
    ) -> Self {
        let metrics = register_metrics(prometheus_registry);
        Self {
            network,
            protocol_name,
            live_peers,
            peers_cache: VecDeque::new(),
            peers_revision: 0,
            state: State::Idle,
            metrics,
        }
    }

    fn reset_peers_cache_for_block(&mut self, block: NumberFor<B>) {
        let peers = self.live_peers.lock();
        self.peers_cache = peers.further_than(block);
        self.peers_revision = peers.progress_revision();
    }

    fn try_next_peer(&mut self) -> Option<PeerId> {
        let live = self.live_peers.lock();
        while let Some(peer) = self.peers_cache.pop_front() {
            if live.contains(&peer) {
                return Some(peer);
            }
        }
        None
    }

    fn request_from_peer(&mut self, peer: PeerId, req_info: RequestInfo<B, AuthorityId>) {
        debug!(
            target: BEEFY_SYNC_LOG_TARGET,
            "🥩 requesting justif #{:?} from peer {:?}", req_info.block, peer,
        );

        let payload = JustificationRequest::<B> {
            begin: req_info.block,
        }
        .encode();

        let (tx, rx) = oneshot::channel();

        self.network.start_request(
            peer,
            self.protocol_name.clone(),
            payload,
            None,
            tx,
            IfDisconnected::ImmediateError,
        );

        self.state = State::AwaitingResponse(peer, req_info, rx);
    }

    /// Start new justification request for `block`, if no other request is in progress.
    ///
    /// `active_set` will be used to verify validity of potential responses.
    pub fn request(&mut self, block: NumberFor<B>, active_set: ValidatorSet<AuthorityId>) {
        // ignore new requests while there's already one pending
        if matches!(self.state, State::AwaitingResponse(_, _, _)) {
            return;
        }
        if let State::WaitingForPeers(req_info) = &self.state {
            if req_info.block == block
                && req_info.active_set == active_set
                && self.live_peers.lock().progress_revision() == self.peers_revision
            {
                return;
            }
        }
        self.reset_peers_cache_for_block(block);

        // Start the requests engine - each unsuccessful received response will automatically
        // trigger a new request to the next peer in the `peers_cache` until there are none left.
        if let Some(peer) = self.try_next_peer() {
            self.request_from_peer(peer, RequestInfo { block, active_set });
        } else {
            self.state = State::WaitingForPeers(RequestInfo { block, active_set });
            metric_inc!(
                self.metrics,
                beefy_on_demand_justification_no_peer_to_request_from
            );
            debug!(
                target: BEEFY_SYNC_LOG_TARGET,
                "🥩 no good peers to request justif #{:?} from", block
            );
        }
    }

    /// Cancel any pending request for block numbers smaller or equal to `block`.
    pub fn cancel_requests_older_than(&mut self, block: NumberFor<B>) {
        match &self.state {
            State::AwaitingResponse(_, req_info, _) | State::WaitingForPeers(req_info)
                if req_info.block <= block =>
            {
                debug!(
                    target: BEEFY_SYNC_LOG_TARGET,
                    "🥩 cancel pending request for justification #{:?}", req_info.block
                );
                self.state = State::Idle;
            }
            _ => (),
        }
    }

    fn process_response(
        &mut self,
        peer: &PeerId,
        req_info: &RequestInfo<B, AuthorityId>,
        response: Result<Response, Canceled>,
    ) -> Result<BeefyVersionedFinalityProof<B, AuthorityId>, Error> {
        response
            .map_err(|e| {
                debug!(
                    target: BEEFY_SYNC_LOG_TARGET,
                    "🥩 on-demand sc-network channel sender closed, err: {:?}", e
                );
                Error::ResponseError
            })?
            .map_err(|e| {
                debug!(
                    target: BEEFY_SYNC_LOG_TARGET,
                    "🥩 for on demand justification #{:?}, peer {:?} error: {:?}",
                    req_info.block,
                    peer,
                    e
                );
                match e {
                    RequestFailure::Refused => {
                        metric_inc!(self.metrics, beefy_on_demand_justification_peer_refused);
                        let peer_report = PeerReport {
                            who: *peer,
                            cost_benefit: cost::REFUSAL_RESPONSE,
                        };
                        Error::InvalidResponse(peer_report)
                    }
                    _ => {
                        metric_inc!(self.metrics, beefy_on_demand_justification_peer_error);
                        Error::ResponseError
                    }
                }
            })
            .and_then(|(encoded, _)| {
                decode_and_verify_finality_proof::<B, AuthorityId>(
					&encoded[..],
					req_info.block,
					&req_info.active_set,
				)
				.map_err(|(err, signatures_checked)| {
					metric_inc!(self.metrics, beefy_on_demand_justification_invalid_proof);
					debug!(
						target: BEEFY_SYNC_LOG_TARGET,
						"🥩 for on demand justification #{:?}, peer {:?} responded with invalid proof: {:?}",
						req_info.block, peer, err
					);
					let mut cost = cost::INVALID_PROOF;
					cost.value +=
						cost::PER_SIGNATURE_CHECKED.saturating_mul(signatures_checked as i32);
					Error::InvalidResponse(PeerReport { who: *peer, cost_benefit: cost })
				})
            })
    }

    pub(crate) async fn next(&mut self) -> ResponseInfo<B, AuthorityId> {
        let (peer, req_info, resp) = loop {
            match &mut self.state {
                State::Idle => {
                    futures::future::pending::<()>().await;
                    return ResponseInfo::Pending;
                }
                State::WaitingForPeers(_) => {
                    futures::future::poll_fn(|cx| {
                        self.live_peers
                            .lock()
                            .poll_progress(cx, self.peers_revision)
                    })
                    .await;
                    let State::WaitingForPeers(req_info) =
                        std::mem::replace(&mut self.state, State::Idle)
                    else {
                        unreachable!("exclusive polling preserves the waiting request");
                    };
                    self.request(req_info.block, req_info.active_set);
                }
                State::AwaitingResponse(peer, req_info, receiver) => {
                    let resp = receiver.await;
                    break (*peer, req_info.clone(), resp);
                }
            }
        };
        // We received the awaited response. Our 'receiver' will never generate any other response,
        // meaning we're done with current state. Move the engine to `State::Idle`.
        self.state = State::Idle;

        let block = req_info.block;
        match self.process_response(&peer, &req_info, resp) {
            Err(err) => {
                // No valid justification received, try next peer in our set.
                if let Some(peer) = self.try_next_peer() {
                    self.request_from_peer(peer, req_info);
                } else {
                    // Stay idle so the worker can retry without a newer peer progress hint.
                    metric_inc!(
                        self.metrics,
                        beefy_on_demand_justification_no_peer_to_request_from
                    );

                    let num_cache = self.peers_cache.len();
                    let num_live = self.live_peers.lock().len();
                    warn!(
                        target: BEEFY_SYNC_LOG_TARGET,
                        "🥩 ran out of peers to request justif #{block:?} from num_cache={num_cache} num_live={num_live} err={err:?}",
                    );
                }
                // Report peer based on error type.
                if let Error::InvalidResponse(peer_report) = err {
                    ResponseInfo::PeerReport(peer_report)
                } else {
                    ResponseInfo::Pending
                }
            }
            Ok(proof) => {
                metric_inc!(self.metrics, beefy_on_demand_justification_good_proof);
                debug!(
                    target: BEEFY_SYNC_LOG_TARGET,
                    "🥩 received valid on-demand justif #{block:?} from {peer:?}",
                );
                let peer_report = PeerReport {
                    who: peer,
                    cost_benefit: benefit::VALIDATED_PROOF,
                };
                ResponseInfo::ValidProof(proof, peer_report)
            }
        }
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use crate::{justification::tests::new_finality_proof, tests::make_beefy_ids};
    use futures::{
        channel::mpsc,
        task::{waker, ArcWake},
        Future,
    };
    use sp_consensus_beefy::{ecdsa_crypto, test_utils::Keyring};
    use std::{
        sync::atomic::{AtomicUsize, Ordering},
        task::{Context, Poll},
    };
    use substrate_test_runtime_client::runtime::Block;

    type PendingRequest = (PeerId, Vec<u8>, oneshot::Sender<Response>);

    pub(crate) struct RequestNetwork(pub(crate) mpsc::UnboundedSender<PendingRequest>);

    #[async_trait::async_trait]
    impl NetworkRequest for RequestNetwork {
        async fn request(
            &self,
            _: PeerId,
            _: ProtocolName,
            _: Vec<u8>,
            _: Option<(Vec<u8>, ProtocolName)>,
            _: IfDisconnected,
        ) -> Response {
            unimplemented!("the on-demand engine uses start_request")
        }

        fn start_request(
            &self,
            peer: PeerId,
            _: ProtocolName,
            request: Vec<u8>,
            _: Option<(Vec<u8>, ProtocolName)>,
            response: oneshot::Sender<Response>,
            _: IfDisconnected,
        ) {
            self.0.unbounded_send((peer, request, response)).unwrap();
        }
    }

    #[derive(Default)]
    struct WakeCounter(AtomicUsize);

    impl ArcWake for WakeCounter {
        fn wake_by_ref(this: &Arc<Self>) {
            this.0.fetch_add(1, Ordering::Relaxed);
        }
    }

    #[test]
    fn peer_progress_hint_retries_mandatory_request_without_new_finality() {
        let keys = [Keyring::<ecdsa_crypto::AuthorityId>::Alice];
        let active_set = ValidatorSet::new(make_beefy_ids(&keys), 7).unwrap();
        let known_peers = Arc::new(Mutex::new(KnownPeers::<Block>::new()));
        let (requests_tx, mut requests_rx) = mpsc::unbounded();
        let protocol: ProtocolName = "/beefy/justifs/1".into();
        let mut engine = OnDemandJustificationsEngine::new(
            Arc::new(RequestNetwork(requests_tx)),
            protocol.clone(),
            known_peers.clone(),
            None,
        );
        let wakes = Arc::new(WakeCounter::default());
        let task_waker = waker(wakes.clone());
        let mut cx = Context::from_waker(&task_waker);
        engine.request(5, active_set.clone());
        assert!(std::pin::pin!(engine.next()).poll(&mut cx).is_pending());
        assert!(requests_rx.try_next().is_err());

        let peer = PeerId::random();
        known_peers.lock().note_vote_for(peer, 5);
        assert_eq!(wakes.0.load(Ordering::Relaxed), 1);
        assert!(std::pin::pin!(engine.next()).poll(&mut cx).is_pending());
        assert!(requests_rx.try_next().is_err());
        known_peers.lock().note_vote_for(peer, 20);
        assert_eq!(wakes.0.load(Ordering::Relaxed), 2);
        assert!(std::pin::pin!(engine.next()).poll(&mut cx).is_pending());
        let (requested_peer, request, response) = requests_rx.try_next().unwrap().unwrap();
        assert_eq!(requested_peer, peer);
        assert_eq!(request, JustificationRequest::<Block> { begin: 5 }.encode());
        response.send(Ok((vec![0xff], protocol.clone()))).unwrap();
        assert!(matches!(
            std::pin::pin!(engine.next()).poll(&mut cx),
            Poll::Ready(ResponseInfo::PeerReport(_))
        ));

        engine.request(5, active_set.clone());
        let (_, request, response) = requests_rx.try_next().unwrap().unwrap();
        engine.request(5, active_set.clone());
        assert!(requests_rx.try_next().is_err());
        assert_eq!(request, JustificationRequest::<Block> { begin: 5 }.encode());
        let proof = new_finality_proof(5, &active_set, &keys);
        response.send(Ok((proof.encode(), protocol))).unwrap();
        match std::pin::pin!(engine.next()).poll(&mut cx) {
            Poll::Ready(ResponseInfo::ValidProof(received, _)) => assert_eq!(received, proof),
            _ => panic!("the retained historical validator set must verify the response"),
        }

        known_peers.lock().remove(&peer);
        engine.request(10, active_set);
        assert!(std::pin::pin!(engine.next()).poll(&mut cx).is_pending());
        engine.cancel_requests_older_than(10);
        known_peers.lock().note_vote_for(peer, 30);
        assert!(std::pin::pin!(engine.next()).poll(&mut cx).is_pending());
        assert!(requests_rx.try_next().is_err());
    }

    #[test]
    fn failed_request_retries_without_new_peer_progress() {
        for error in [
            Some(RequestFailure::Network(
                sc_network::request_responses::OutboundFailure::Timeout,
            )),
            None,
        ] {
            let keys = [Keyring::<ecdsa_crypto::AuthorityId>::Alice];
            let active_set = ValidatorSet::new(make_beefy_ids(&keys), 7).unwrap();
            let known_peers = Arc::new(Mutex::new(KnownPeers::<Block>::new()));
            let peer = PeerId::random();
            known_peers.lock().note_vote_for(peer, 20);
            let (requests_tx, mut requests_rx) = mpsc::unbounded();
            let protocol: ProtocolName = "/beefy/justifs/1".into();
            let mut engine = OnDemandJustificationsEngine::new(
                Arc::new(RequestNetwork(requests_tx)),
                protocol.clone(),
                known_peers,
                None,
            );
            engine.request(5, active_set.clone());
            let (_, _, response) = requests_rx.try_next().unwrap().unwrap();
            if let Some(error) = error {
                response.send(Err(error)).unwrap();
            } else {
                drop(response);
            }
            assert!(matches!(
                futures::executor::block_on(engine.next()),
                ResponseInfo::Pending
            ));

            // The worker retries the mandatory target after handling the failed response.
            engine.request(5, active_set.clone());
            let (requested_peer, request, response) = requests_rx
                .try_next()
                .expect("a failed request must not require a newer peer progress hint")
                .unwrap();
            assert_eq!(requested_peer, peer);
            assert_eq!(request, JustificationRequest::<Block> { begin: 5 }.encode());
            let proof = new_finality_proof(5, &active_set, &keys);
            response.send(Ok((proof.encode(), protocol))).unwrap();
            match futures::executor::block_on(engine.next()) {
                ResponseInfo::ValidProof(received, _) => assert_eq!(received, proof),
                _ => panic!("the retried historical proof must verify against the requested set"),
            }
        }
    }
}
