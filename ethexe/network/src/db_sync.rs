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
    export::{Multiaddr, PeerId},
    utils::ParityScaleCodec,
};
use ethexe_db::{BlockMetaStorage, CodesStorage, Database};
use gear_core::ids::ProgramId;
use gprimitives::{ActorId, CodeId, H256};
use libp2p::{
    core::Endpoint,
    futures::FutureExt,
    request_response,
    request_response::{Message, ProtocolSupport},
    swarm::{
        behaviour::ConnectionEstablished, ConnectionClosed, ConnectionDenied, ConnectionId,
        FromSwarm, NetworkBehaviour, THandler, THandlerInEvent, THandlerOutEvent, ToSwarm,
    },
    StreamProtocol,
};
use parity_scale_codec::{Decode, Encode};
use std::{
    collections::{BTreeMap, BTreeSet, HashMap, HashSet},
    task::{Context, Poll},
};
use tokio::task::JoinHandle;

const STREAM_PROTOCOL: StreamProtocol =
    StreamProtocol::new(concat!("/ethexe/db-sync/", env!("CARGO_PKG_VERSION")));

type BlockEndProgramStates = BTreeMap<ActorId, H256>;

type DataForHashesKeys = BTreeSet<H256>;
type DataForHashes = BTreeMap<H256, Vec<u8>>;

type ProgramCodeIds = BTreeMap<ProgramId, CodeId>;

#[derive(Debug, Encode, Decode)]
pub enum Request {
    BlockEndProgramStates(H256),
    DataForHashes(DataForHashesKeys),
    ProgramCodeIds(Vec<ProgramId>),
}

#[derive(Debug, Eq, PartialEq, Encode, Decode)]
pub enum Response {
    BlockEndProgramStates {
        /// Block hash states requested for
        block_hash: H256,
        /// Program states for request block
        states: BlockEndProgramStates,
    },
    /// Key (hash) - value (bytes) data
    DataForHashes(DataForHashes),
    /// Program IDs and their corresponding code IDs
    ProgramCodeIds(ProgramCodeIds),
}

#[derive(Debug, Eq, PartialEq)]
pub enum Event {
    RequestSucceed {
        /// Peer who responded to data request
        peer_id: PeerId,
        /// Response itself
        response: Response,
    },
}

type InnerBehaviour = request_response::Behaviour<ParityScaleCodec<Request, Response>>;

pub(crate) struct Behaviour {
    inner: InnerBehaviour,
    user_requests: Vec<Request>,
    db: Database,
    db_reader: Option<(
        request_response::ResponseChannel<Response>,
        JoinHandle<Response>,
    )>,
    connections: HashMap<PeerId, HashSet<ConnectionId>>,
}

impl Behaviour {
    pub fn new(cfg: request_response::Config, db: Database) -> Self {
        Self {
            inner: InnerBehaviour::new([(STREAM_PROTOCOL, ProtocolSupport::Full)], cfg),
            user_requests: Vec::new(),
            db,
            db_reader: None,
            connections: HashMap::new(),
        }
    }

    pub fn request(&mut self, request: Request) {
        self.user_requests.push(request);
    }

    fn read_db(&self, request: Request) -> JoinHandle<Response> {
        let db = self.db.clone();
        tokio::task::spawn_blocking(move || match request {
            Request::BlockEndProgramStates(block_hash) => Response::BlockEndProgramStates {
                block_hash,
                states: db.block_end_program_states(block_hash).unwrap_or_default(),
            },
            Request::DataForHashes(hashes) => Response::DataForHashes(
                hashes
                    .into_iter()
                    .filter_map(|hash| Some((hash, db.read_by_hash(hash)?)))
                    .collect(),
            ),
            Request::ProgramCodeIds(ids) => Response::ProgramCodeIds(
                ids.into_iter()
                    .filter_map(|program_id| Some((program_id, db.program_code_id(program_id)?)))
                    .collect(),
            ),
        })
    }

    fn handle_inner_event(
        &mut self,
        event: request_response::Event<Request, Response>,
    ) -> Poll<ToSwarm<Event, THandlerInEvent<Self>>> {
        match event {
            request_response::Event::Message {
                peer: _,
                message:
                    Message::Request {
                        request_id: _,
                        request,
                        channel,
                    },
            } => {
                self.db_reader = Some((channel, self.read_db(request)));
            }
            request_response::Event::Message {
                peer,
                message:
                    Message::Response {
                        request_id: _,
                        response,
                    },
            } => {
                let event = Event::RequestSucceed {
                    peer_id: peer,
                    response,
                };
                return Poll::Ready(ToSwarm::GenerateEvent(event));
            }
            request_response::Event::OutboundFailure { .. } => {}
            request_response::Event::InboundFailure { .. } => {}
            request_response::Event::ResponseSent { .. } => {}
        }

        Poll::Pending
    }
}

impl NetworkBehaviour for Behaviour {
    type ConnectionHandler = THandler<InnerBehaviour>;
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
    ) -> Result<THandler<Self>, ConnectionDenied> {
        self.inner
            .handle_established_outbound_connection(connection_id, peer, addr, role_override)
    }

    fn on_swarm_event(&mut self, event: FromSwarm) {
        self.inner.on_swarm_event(event);

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
        // TODO: way to choose peer
        if let Some(peer_id) = self.connections.keys().next() {
            for request in self.user_requests.drain(..) {
                let _outbound_request_id = self.inner.send_request(peer_id, request);
            }
        }

        if let Some((channel, mut db_reader)) = self.db_reader.take() {
            if let Poll::Ready(data) = db_reader.poll_unpin(cx) {
                // TODO: check request kind corresponds to response kind
                let resp = data.expect("database panicked");
                let _res = self.inner.send_response(channel, resp);
            } else {
                self.db_reader = Some((channel, db_reader));
            }
        }

        if let Poll::Ready(to_swarm) = self.inner.poll(cx) {
            return match to_swarm {
                ToSwarm::GenerateEvent(event) => self.handle_inner_event(event),
                to_swarm => Poll::Ready(to_swarm.map_out::<Event>(|_event| {
                    unreachable!("`ToSwarm::GenerateEvent` is handled above")
                })),
            };
        }

        Poll::Pending
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::utils::tests::init_logger;
    use ethexe_db::MemDb;
    use libp2p::Swarm;
    use libp2p_swarm_test::SwarmExt;

    fn new_swarm() -> (Swarm<Behaviour>, Database) {
        let db = Database::from_one(&MemDb::default());
        let behaviour = Behaviour::new(request_response::Config::default(), db.clone());
        let swarm = Swarm::new_ephemeral(move |_keypair| behaviour);
        (swarm, db)
    }

    #[tokio::test]
    async fn smoke() {
        init_logger();

        let (mut alice, alice_db) = new_swarm();
        let (mut bob, bob_db) = new_swarm();
        bob.listen().with_memory_addr_external().await;
        let bob_id = *bob.local_peer_id();

        let hello_hash = bob_db.write(b"hello");
        let world_hash = bob_db.write(b"world");

        alice.connect(&mut bob).await;

        tokio::spawn(bob.loop_on_next());

        alice
            .behaviour_mut()
            .request(Request::DataForHashes([hello_hash, world_hash].into()));

        let event = alice.next_behaviour_event().await;
        assert_eq!(
            event,
            Event::RequestSucceed {
                peer_id: bob_id,
                response: Response::DataForHashes(
                    [
                        (hello_hash, b"hello".to_vec()),
                        (world_hash, b"world".to_vec())
                    ]
                    .into()
                )
            }
        )
    }
}
