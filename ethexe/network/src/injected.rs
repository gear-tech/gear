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
    db_sync::{Multiaddr, PeerId},
    utils::ParityScaleCodec,
    validator::discovery::ValidatorIdentities,
};
use ethexe_common::{
    Address, HashOf,
    injected::{
        AddressedInjectedTransaction, InjectedTransaction, InjectedTransactionAcceptance,
        SignedInjectedTransaction,
    },
};
use futures::{FutureExt, StreamExt, future::BoxFuture, stream::FuturesUnordered};
use libp2p::{
    StreamProtocol,
    core::{Endpoint, transport::PortUse},
    request_response,
    request_response::{
        InboundFailure, Message, OutboundFailure, OutboundRequestId, ProtocolSupport,
        ResponseChannel,
    },
    swarm::{
        ConnectionDenied, ConnectionId, FromSwarm, NetworkBehaviour, THandler, THandlerInEvent,
        THandlerOutEvent, ToSwarm,
    },
};
use lru::LruCache;
use parity_scale_codec::{Decode, Encode};
use std::{
    collections::HashMap,
    num::NonZeroUsize,
    task::{Context, Poll, ready},
};
use tokio::sync::oneshot;

const STREAM_PROTOCOL: StreamProtocol = StreamProtocol::new("/ethexe/injected-tx/1.0.0");

/// The maximum number of concurrent requests is allowed to be handled
const MAX_PENDING_REQUESTS: NonZeroUsize = NonZeroUsize::new(20).unwrap();
/// Maximum number of transactions we cache to deny double submission
const MAX_TRANSACTIONS: NonZeroUsize = NonZeroUsize::new(50).unwrap();
/// Maximum number of validators we cache per transaction
const MAX_VALIDATORS_PER_TRANSACTION: NonZeroUsize = NonZeroUsize::new(20).unwrap();

#[derive(Clone, metrics_derive::Metrics)]
#[metrics(scope = "ethexe_network_injected")]
struct Metrics {
    /// Number of injected transactions sent to a validator
    sent_transactions: metrics::Counter,
    /// Number of injected transactions received from a user
    received_transactions: metrics::Counter,
}

impl Metrics {
    fn record(&self, event: &Event) {
        match event {
            Event::InboundTransaction { .. } => {
                self.received_transactions.increment(1);
            }
            Event::OutboundAcceptance { .. } => {
                self.sent_transactions.increment(1);
            }
        }
    }
}

/// Network-only type to be encoded-decoded and sent over the network
#[derive(Debug, Encode, Decode)]
pub(crate) struct InnerRequest(SignedInjectedTransaction);

/// Network-only type to be encoded-decoded and sent over the network
#[derive(Debug, Encode, Decode)]
pub(crate) struct InnerResponse(InjectedTransactionAcceptance);

#[derive(Debug)]
pub enum Event {
    /// Peer sent a new transaction to us
    InboundTransaction {
        transaction: SignedInjectedTransaction,
        channel: oneshot::Sender<InjectedTransactionAcceptance>,
    },
    /// We got a response from a validator we sent transaction to
    OutboundAcceptance {
        transaction_hash: HashOf<InjectedTransaction>,
        acceptance: InjectedTransactionAcceptance,
    },
}

#[cfg(test)]
impl Event {
    fn unwrap_new_injected_transaction(
        self,
    ) -> (
        SignedInjectedTransaction,
        oneshot::Sender<InjectedTransactionAcceptance>,
    ) {
        match self {
            Event::InboundTransaction {
                transaction,
                channel,
            } => (transaction, channel),
            _ => panic!("Expected InboundTransaction event"),
        }
    }

    fn unwrap_injected_transaction_acceptance(
        self,
    ) -> (HashOf<InjectedTransaction>, InjectedTransactionAcceptance) {
        match self {
            Event::OutboundAcceptance {
                transaction_hash,
                acceptance,
            } => (transaction_hash, acceptance),
            _ => panic!("Expected OutboundAcceptance event"),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, derive_more::Display)]
pub enum SendTransactionError {
    #[display("too many pending requests")]
    TooManyPendingRequests,
    #[display("transaction already sent")]
    TransactionAlreadySent,
    #[display("validator not found")]
    ValidatorNotFound,
}

type InnerBehaviour = request_response::Behaviour<ParityScaleCodec<InnerRequest, InnerResponse>>;
type PendingResponseFuture = BoxFuture<'static, (ResponseChannel<InnerResponse>, InnerResponse)>;

pub(crate) struct Behaviour {
    inner: InnerBehaviour,
    pending_requests: HashMap<OutboundRequestId, HashOf<InjectedTransaction>>,
    pending_responses: FuturesUnordered<PendingResponseFuture>,
    transaction_cache: LruCache<HashOf<InjectedTransaction>, LruCache<Address, ()>>,
    metrics: Metrics,
}

impl Behaviour {
    pub fn new() -> Self {
        let inner = request_response::Behaviour::new(
            [(STREAM_PROTOCOL, ProtocolSupport::Full)],
            request_response::Config::default(),
        );
        Self {
            inner,
            pending_requests: HashMap::new(),
            pending_responses: FuturesUnordered::new(),
            transaction_cache: LruCache::new(MAX_TRANSACTIONS),
            metrics: Metrics::default(),
        }
    }

    pub fn send_transaction(
        &mut self,
        identities: &ValidatorIdentities,
        transaction: AddressedInjectedTransaction,
    ) -> Result<(), SendTransactionError> {
        let AddressedInjectedTransaction { recipient, tx } = transaction;
        let tx_hash = tx.data().to_hash();

        if self.pending_requests.len() >= MAX_PENDING_REQUESTS.get() {
            return Err(SendTransactionError::TooManyPendingRequests);
        }

        if let Some(transactions) = self.transaction_cache.get_mut(&tx_hash)
            && let Some(&()) = transactions.get(&recipient)
        {
            return Err(SendTransactionError::TransactionAlreadySent);
        }

        let identity = identities
            .get(&recipient)
            .ok_or(SendTransactionError::ValidatorNotFound)?;
        let peer_id = identity.peer_id();
        let addresses = identity.addresses().iter().cloned().collect();

        let id = self
            .inner
            .send_request_with_addresses(&peer_id, InnerRequest(tx), addresses);
        self.pending_requests.insert(id, tx_hash);

        self.transaction_cache
            .get_or_insert_mut(tx_hash, || LruCache::new(MAX_VALIDATORS_PER_TRANSACTION))
            .put(recipient, ());

        Ok(())
    }

    fn handle_inner_event(
        &mut self,
        event: request_response::Event<InnerRequest, InnerResponse>,
    ) -> Poll<Event> {
        match event {
            request_response::Event::Message {
                peer: _,
                connection_id: _,
                message:
                    Message::Request {
                        request_id: _,
                        request,
                        channel,
                    },
            } => {
                let InnerRequest(transaction) = request;
                let (tx, rx) = oneshot::channel();

                let fut = async {
                    let acceptance = rx.await.expect("channel must never be dropped");
                    (channel, InnerResponse(acceptance))
                };
                self.pending_responses.push(fut.boxed());

                return Poll::Ready(Event::InboundTransaction {
                    transaction,
                    channel: tx,
                });
            }
            request_response::Event::Message {
                peer: _,
                connection_id: _,
                message:
                    Message::Response {
                        request_id,
                        response,
                    },
            } => {
                let transaction_hash = self
                    .pending_requests
                    .remove(&request_id)
                    .expect("unknown request id");

                let InnerResponse(acceptance) = response;
                return Poll::Ready(Event::OutboundAcceptance {
                    transaction_hash,
                    acceptance,
                });
            }
            request_response::Event::OutboundFailure {
                peer,
                connection_id: _,
                request_id,
                error,
            } => {
                let transaction_hash = self
                    .pending_requests
                    .remove(&request_id)
                    .expect("unknown request id");

                if let OutboundFailure::UnsupportedProtocols = error {
                    log::debug!(
                        "request to {peer} failed because it doesn't support {STREAM_PROTOCOL} protocol"
                    );
                }

                let acceptance = Err(error.to_string()).into();
                return Poll::Ready(Event::OutboundAcceptance {
                    transaction_hash,
                    acceptance,
                });
            }
            request_response::Event::InboundFailure {
                peer,
                connection_id: _,
                request_id: _,
                error: InboundFailure::UnsupportedProtocols,
            } => {
                log::debug!(
                    "request from {peer} failed because it doesn't support {STREAM_PROTOCOL} protocol"
                );
            }
            request_response::Event::InboundFailure { .. } => {}
            request_response::Event::ResponseSent { .. } => {}
        }

        Poll::Pending
    }
}

impl NetworkBehaviour for Behaviour {
    type ConnectionHandler = <InnerBehaviour as NetworkBehaviour>::ConnectionHandler;
    type ToSwarm = Event;

    fn handle_pending_inbound_connection(
        &mut self,
        connection_id: ConnectionId,
        local_addr: &Multiaddr,
        remote_addr: &Multiaddr,
    ) -> Result<(), ConnectionDenied> {
        self.inner
            .handle_pending_inbound_connection(connection_id, local_addr, remote_addr)
    }

    fn handle_established_inbound_connection(
        &mut self,
        connection_id: ConnectionId,
        peer: PeerId,
        local_addr: &Multiaddr,
        remote_addr: &Multiaddr,
    ) -> Result<THandler<Self>, ConnectionDenied> {
        self.inner.handle_established_inbound_connection(
            connection_id,
            peer,
            local_addr,
            remote_addr,
        )
    }

    fn handle_pending_outbound_connection(
        &mut self,
        connection_id: ConnectionId,
        maybe_peer: Option<PeerId>,
        addresses: &[Multiaddr],
        effective_role: Endpoint,
    ) -> Result<Vec<Multiaddr>, ConnectionDenied> {
        self.inner.handle_pending_outbound_connection(
            connection_id,
            maybe_peer,
            addresses,
            effective_role,
        )
    }

    fn handle_established_outbound_connection(
        &mut self,
        connection_id: ConnectionId,
        peer: PeerId,
        addr: &Multiaddr,
        role_override: Endpoint,
        port_use: PortUse,
    ) -> Result<THandler<Self>, ConnectionDenied> {
        self.inner.handle_established_outbound_connection(
            connection_id,
            peer,
            addr,
            role_override,
            port_use,
        )
    }

    fn on_swarm_event(&mut self, event: FromSwarm) {
        self.inner.on_swarm_event(event)
    }

    fn on_connection_handler_event(
        &mut self,
        peer_id: PeerId,
        connection_id: ConnectionId,
        event: THandlerOutEvent<Self>,
    ) {
        self.inner
            .on_connection_handler_event(peer_id, connection_id, event)
    }

    fn poll(
        &mut self,
        cx: &mut Context<'_>,
    ) -> Poll<ToSwarm<Self::ToSwarm, THandlerInEvent<Self>>> {
        if let Poll::Ready(Some((channel, response))) = self.pending_responses.poll_next_unpin(cx) {
            let _res = self.inner.send_response(channel, response);
        }

        let to_swarm = ready!(self.inner.poll(cx));
        match to_swarm {
            ToSwarm::GenerateEvent(event) => self
                .handle_inner_event(event)
                .map(|event| {
                    self.metrics.record(&event);
                    event
                })
                .map(ToSwarm::GenerateEvent),
            to_swarm => Poll::Ready(to_swarm.map_out::<Event>(|_event| {
                unreachable!("`ToSwarm::GenerateEvent` is handled above")
            })),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        utils::tests::init_logger,
        validator::discovery::{SignedValidatorIdentity, ValidatorAddresses, ValidatorIdentity},
    };
    use ethexe_common::{injected::InjectedTransaction, mock::Mock};
    use gsigner::secp256k1::{Secp256k1SignerExt, Signer};
    use libp2p::{
        Swarm, Transport,
        core::{transport::MemoryTransport, upgrade::Version},
        identity::Keypair,
    };
    use libp2p_swarm_test::SwarmExt;
    use std::time::Duration;

    fn addressed_injected_tx(recipient: Address) -> AddressedInjectedTransaction {
        let signer = Signer::memory();
        let pub_key = signer.generate().unwrap();

        let tx = InjectedTransaction::mock(());
        let tx = signer.signed_message(pub_key, tx, None).unwrap();

        AddressedInjectedTransaction { recipient, tx }
    }

    async fn new_swarm() -> (Swarm<Behaviour>, SignedValidatorIdentity) {
        // code exactly from `Swarm::new_ephemeral_tokio` but with SECP256K1 keypair
        let keypair = Keypair::generate_secp256k1();
        let peer_id = PeerId::from(keypair.public());

        let transport = MemoryTransport::default()
            .or_transport(libp2p::tcp::tokio::Transport::default())
            .upgrade(Version::V1)
            .authenticate(libp2p::plaintext::Config::new(&keypair))
            .multiplex(libp2p::yamux::Config::default())
            .timeout(Duration::from_secs(20))
            .boxed();

        let mut swarm = Swarm::new(
            transport,
            Behaviour::new(),
            peer_id,
            libp2p::swarm::Config::with_tokio_executor(),
        );

        let (mem_addr, _tcp_addr) = swarm.listen().with_memory_addr_external().await;

        let signer = Signer::memory();
        let pub_key = signer.generate().unwrap();

        let identity = ValidatorIdentity {
            addresses: ValidatorAddresses::new(*swarm.local_peer_id(), mem_addr),
            creation_time: 0,
        };
        let identity = identity.sign(&signer, pub_key, &keypair).unwrap();

        (swarm, identity)
    }

    #[tokio::test]
    async fn accept() {
        init_logger();

        let (mut alice, _) = new_swarm().await;
        let (mut bob, bob_identity) = new_swarm().await;

        let transaction = addressed_injected_tx(bob_identity.address());
        let identities = [(bob_identity.address(), bob_identity)].into();

        alice
            .behaviour_mut()
            .send_transaction(&identities, transaction.clone())
            .unwrap();
        let alice_handle = tokio::spawn(async move {
            let (_tx_hash, acceptance) = alice
                .next_behaviour_event()
                .await
                .unwrap_injected_transaction_acceptance();
            assert_eq!(acceptance, InjectedTransactionAcceptance::Accept);
        });

        let (new_tx, channel) = bob
            .next_behaviour_event()
            .await
            .unwrap_new_injected_transaction();
        assert_eq!(new_tx, transaction.tx);
        channel.send(InjectedTransactionAcceptance::Accept).unwrap();
        tokio::spawn(bob.loop_on_next());

        alice_handle.await.unwrap();
    }

    #[tokio::test]
    async fn rejected() {
        const REJECT_REASON: &str = "test reason";

        let (mut alice, _) = new_swarm().await;
        let (mut bob, bob_identity) = new_swarm().await;

        let transaction = addressed_injected_tx(bob_identity.address());
        let identities = [(bob_identity.address(), bob_identity)].into();

        alice
            .behaviour_mut()
            .send_transaction(&identities, transaction.clone())
            .unwrap();
        let alice_handle = tokio::spawn(async move {
            let (_tx_hash, acceptance) = alice
                .next_behaviour_event()
                .await
                .unwrap_injected_transaction_acceptance();
            assert_eq!(
                acceptance,
                InjectedTransactionAcceptance::Reject {
                    reason: REJECT_REASON.to_string(),
                }
            );
        });

        let (new_tx, channel) = bob
            .next_behaviour_event()
            .await
            .unwrap_new_injected_transaction();
        assert_eq!(new_tx, transaction.tx);
        channel
            .send(InjectedTransactionAcceptance::Reject {
                reason: REJECT_REASON.to_string(),
            })
            .unwrap();
        tokio::spawn(bob.loop_on_next());

        alice_handle.await.unwrap();
    }

    #[tokio::test]
    async fn outbound_failure_rejected() {
        let (mut alice, _) = new_swarm().await;
        let (mut bob, bob_identity) = new_swarm().await;

        let transaction = addressed_injected_tx(bob_identity.address());
        let identities = [(bob_identity.address(), bob_identity)].into();

        alice
            .behaviour_mut()
            .send_transaction(&identities, transaction.clone())
            .unwrap();
        let alice_handle = tokio::spawn(async move {
            let (_tx_hash, acceptance) = alice
                .next_behaviour_event()
                .await
                .unwrap_injected_transaction_acceptance();
            assert_eq!(
                acceptance,
                InjectedTransactionAcceptance::Reject {
                    reason: OutboundFailure::ConnectionClosed.to_string(),
                }
            );
        });

        let (new_tx, _channel) = bob
            .next_behaviour_event()
            .await
            .unwrap_new_injected_transaction();
        assert_eq!(new_tx, transaction.tx);
        drop(bob);

        alice_handle.await.unwrap();
    }

    #[tokio::test]
    async fn too_many_pending_requests() {
        init_logger();

        let mut alice = Behaviour::new();
        let (_bob, bob_identity) = new_swarm().await;
        let bob_address = bob_identity.address();

        let identities = [(bob_address, bob_identity)].into();

        for _ in 0..MAX_PENDING_REQUESTS.get() {
            let transaction = addressed_injected_tx(bob_address);
            alice.send_transaction(&identities, transaction).unwrap();
        }

        let transaction = addressed_injected_tx(bob_address);
        let err = alice
            .send_transaction(&identities, transaction)
            .unwrap_err();
        assert_eq!(err, SendTransactionError::TooManyPendingRequests);
    }

    #[tokio::test]
    async fn transaction_already_sent() {
        init_logger();

        let mut alice = Behaviour::new();
        let (_bob, bob_identity) = new_swarm().await;

        let transaction = addressed_injected_tx(bob_identity.address());
        let identities = [(bob_identity.address(), bob_identity)].into();

        alice
            .send_transaction(&identities, transaction.clone())
            .unwrap();

        let err = alice
            .send_transaction(&identities, transaction)
            .unwrap_err();
        assert_eq!(err, SendTransactionError::TransactionAlreadySent);
    }

    #[tokio::test]
    async fn validator_not_found() {
        let mut alice = Behaviour::new();

        let transaction = addressed_injected_tx(Address::default());

        let err = alice
            .send_transaction(&Default::default(), transaction)
            .unwrap_err();
        assert_eq!(err, SendTransactionError::ValidatorNotFound);
    }
}
