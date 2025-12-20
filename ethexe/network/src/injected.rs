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
    peer_score,
    utils::ParityScaleCodec,
    validator,
};
use anyhow::Context as _;
use ethexe_common::{
    Address, HashOf,
    injected::{AddressedInjectedTransaction, InjectedTransaction, SignedInjectedTransaction},
};
use libp2p::{
    StreamProtocol,
    core::{Endpoint, transport::PortUse},
    request_response,
    request_response::{
        InboundFailure, Message, OutboundFailure, OutboundRequestId, ProtocolSupport,
    },
    swarm::{
        ConnectionDenied, ConnectionId, FromSwarm, NetworkBehaviour, THandler, THandlerInEvent,
        THandlerOutEvent, ToSwarm,
    },
};
use lru::LruCache;
use parity_scale_codec::{Decode, Encode};
use std::{
    collections::HashSet,
    num::NonZeroUsize,
    task::{Context, Poll, ready},
};

const STREAM_PROTOCOL: StreamProtocol =
    StreamProtocol::new(concat!("/ethexe/injected-tx/", env!("CARGO_PKG_VERSION")));
const MAX_PENDING_REQUESTS: NonZeroUsize = NonZeroUsize::new(20).unwrap();
const MAX_VALIDATORS: NonZeroUsize = NonZeroUsize::new(50).unwrap();
const MAX_TRANSACTIONS_PER_VALIDATOR: NonZeroUsize = NonZeroUsize::new(20).unwrap();

#[derive(Debug, Encode, Decode)]
pub(crate) enum Request {
    SubmitTx(SignedInjectedTransaction),
}

#[derive(Debug, Encode, Decode)]
pub(crate) enum TxResponse {
    Accepted,
}

#[derive(Debug, Encode, Decode)]
pub(crate) enum Response {
    TxAccepted(TxResponse),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Event {
    NewInjectedTransaction(SignedInjectedTransaction),
}

type InnerBehaviour = request_response::Behaviour<ParityScaleCodec<Request, Response>>;

pub(crate) struct Behaviour {
    inner: InnerBehaviour,
    peer_score: peer_score::Handle,
    pending_requests: HashSet<OutboundRequestId>,
    transaction_cache: LruCache<Address, LruCache<HashOf<InjectedTransaction>, ()>>,
}

impl Behaviour {
    pub fn new(peer_score: peer_score::Handle) -> Self {
        let inner = request_response::Behaviour::new(
            [(STREAM_PROTOCOL, ProtocolSupport::Full)],
            request_response::Config::default(),
        );
        Self {
            inner,
            peer_score,
            pending_requests: HashSet::new(),
            transaction_cache: LruCache::new(MAX_VALIDATORS),
        }
    }

    pub fn send_transaction(
        &mut self,
        discovery: &validator::discovery::Behaviour,
        transaction: AddressedInjectedTransaction,
    ) -> anyhow::Result<()> {
        let AddressedInjectedTransaction { recipient, tx } = transaction;
        let tx_hash = tx.data().to_hash();

        anyhow::ensure!(
            self.pending_requests.len() < MAX_PENDING_REQUESTS.get(),
            "too many pending transactions"
        );

        if let Some(transactions) = self.transaction_cache.get_mut(&recipient)
            && let Some(&()) = transactions.get(&tx_hash)
        {
            anyhow::bail!("transaction already sent");
        }

        let identity = discovery
            .get_identity(recipient)
            .context("validator not found")?;
        let peer_id = identity.peer_id();
        let addresses = identity.addresses().iter().cloned().collect();

        let id = self
            .inner
            .send_request_with_addresses(&peer_id, Request::SubmitTx(tx), addresses);
        self.pending_requests.insert(id);

        self.transaction_cache
            .get_or_insert_mut(recipient, || LruCache::new(MAX_TRANSACTIONS_PER_VALIDATOR))
            .put(tx_hash, ());

        Ok(())
    }

    fn handle_inner_event(
        &mut self,
        event: request_response::Event<Request, Response>,
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
                return match request {
                    Request::SubmitTx(transaction) => {
                        let _res = self
                            .inner
                            .send_response(channel, Response::TxAccepted(TxResponse::Accepted));
                        Poll::Ready(Event::NewInjectedTransaction(transaction))
                    }
                };
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
                assert!(
                    self.pending_requests.remove(&request_id),
                    "unknown request id"
                );

                let Response::TxAccepted(TxResponse::Accepted) = response;
            }
            request_response::Event::OutboundFailure {
                peer,
                connection_id: _,
                request_id,
                error,
            } => {
                assert!(
                    self.pending_requests.remove(&request_id),
                    "unknown request id"
                );

                if let OutboundFailure::UnsupportedProtocols = error {
                    log::debug!(
                        "request to {peer} failed because it doesn't support {STREAM_PROTOCOL} protocol"
                    );
                    self.peer_score.unsupported_protocol(peer);
                }
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
                self.peer_score.unsupported_protocol(peer);
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
        let to_swarm = ready!(self.inner.poll(cx));
        match to_swarm {
            ToSwarm::GenerateEvent(event) => {
                self.handle_inner_event(event).map(ToSwarm::GenerateEvent)
            }
            to_swarm => Poll::Ready(to_swarm.map_out::<Event>(|_event| {
                unreachable!("`ToSwarm::GenerateEvent` is handled above")
            })),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::utils::tests::init_logger;
    use ethexe_common::{injected::InjectedTransaction, mock::Mock};
    use ethexe_signer::Signer;
    use libp2p::Swarm;
    use libp2p_swarm_test::SwarmExt;

    async fn new_swarm() -> Swarm<Behaviour> {
        let mut swarm =
            Swarm::new_ephemeral_tokio(|_keypair| Behaviour::new(peer_score::Handle::new_test()));
        swarm.listen().with_memory_addr_external().await;
        swarm
    }

    #[tokio::test]
    async fn smoke() {
        init_logger();

        let mut alice = new_swarm().await;
        let mut bob = new_swarm().await;
        alice.connect(&mut bob).await;

        let transaction = InjectedTransaction::mock(());
        let signer = Signer::memory();
        let pub_key = signer.generate_key().unwrap();
        let transaction = signer.signed_message(pub_key, transaction).unwrap();

        // TODO: replace with `Behaviour::send_transaction()` when it works
        alice
            .behaviour_mut()
            .inner
            .send_request(bob.local_peer_id(), Request::SubmitTx(transaction.clone()));
        tokio::spawn(alice.loop_on_next());

        let event = bob.next_behaviour_event().await;
        assert_eq!(event, Event::NewInjectedTransaction(transaction));
    }
}
