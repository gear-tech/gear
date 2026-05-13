// This file is part of Gear.
//
// Copyright (C) 2026 Gear Technologies Inc.
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

use beetswap::multihasher::{Multihasher, MultihasherError};
use blockstore::{block::CidError, cond_send::CondSend};
use cid::{Cid, CidGeneric};
use ethexe_common::{
    Announce, HashOf,
    db::{AnnounceStorageRO, HashStorageRO},
};
use futures::FutureExt;
use gprimitives::H256;
use libp2p::{
    Multiaddr, PeerId,
    core::{Endpoint, transport::PortUse},
    swarm::{
        ConnectionDenied, ConnectionId, FromSwarm, NetworkBehaviour, THandler, THandlerInEvent,
        THandlerOutEvent, ToSwarm,
    },
};
use multihash::Multihash;
use parity_scale_codec::{Decode, Encode};
use std::{
    collections::HashMap,
    mem,
    sync::Arc,
    task::{Context, Poll},
};
use tokio::{
    sync::{mpsc, oneshot},
    task,
};

#[derive(Debug, Copy, Clone, Eq, PartialEq, derive_more::From)]
pub enum Request {
    Hash(H256),
    Announce(HashOf<Announce>),
}

impl Request {
    fn into_cid(self) -> Cid {
        match self {
            Request::Hash(hash) => Cid::new_v1(
                Blockstore::RAW_CODEC,
                Multihash::wrap(Blockstore::BLAKE2B_CODE, hash.as_bytes())
                    .expect("size is always correct"),
            ),
            Request::Announce(hash) => Cid::new_v1(
                Blockstore::ANNOUNCES_CODEC,
                Multihash::wrap(Blockstore::BLAKE2B_CODE, hash.inner().as_bytes())
                    .expect("size is always correct"),
            ),
        }
    }

    fn into_response(self, data: Vec<u8>) -> Response {
        match self {
            Request::Hash(_) => Response::Hash(data),
            Request::Announce(_) => {
                Response::Announce(Announce::decode(&mut data.as_slice()).expect("valid announce"))
            }
        }
    }
}

#[derive(Debug, Clone, Eq, PartialEq, derive_more::Unwrap)]
pub enum Response {
    Hash(Vec<u8>),
    Announce(Announce),
}

#[derive(Clone)]
pub struct Handle(mpsc::UnboundedSender<(Request, oneshot::Sender<Response>)>);

impl Handle {
    pub async fn request(&self, request: impl Into<Request>) -> Response {
        let (tx, rx) = oneshot::channel();

        self.0
            .send((request.into(), tx))
            .expect("channel should never be closed");

        rx.await.expect("channel should never be closed")
    }
}

pub(crate) trait BlockstoreDatabase:
    Send + Sync + HashStorageRO + AnnounceStorageRO
{
    fn clone_boxed(&self) -> Box<dyn BlockstoreDatabase>;
}

impl BlockstoreDatabase for ethexe_db::Database {
    fn clone_boxed(&self) -> Box<dyn BlockstoreDatabase> {
        Box::new(self.clone())
    }
}

pub struct Blockstore {
    db: Box<dyn BlockstoreDatabase>,
}

impl Blockstore {
    const MAX_BLOCK_SIZE: u64 = 1024 * 1024; // 1MB
    const BLAKE2B_CODE: u64 = 0xb220;
    const RAW_CODEC: u64 = 0x55;
    const ANNOUNCES_CODEC: u64 = 0x300000;

    fn convert_multihash<const S: usize>(multihash: &Multihash<S>) -> blockstore::Result<H256> {
        let hash: Multihash<32> =
            beetswap::utils::convert_multihash(multihash).ok_or(blockstore::Error::CidTooLarge)?;
        if hash.code() != Self::BLAKE2B_CODE {
            return Err(blockstore::Error::CidError(CidError::InvalidMultihashCode(
                hash.code(),
                Self::BLAKE2B_CODE,
            )));
        }
        if hash.size() as usize != mem::size_of::<H256>() {
            return Err(blockstore::Error::CidError(
                CidError::InvalidMultihashLength(hash.size() as usize),
            ));
        }
        Ok(H256::from_slice(hash.digest()))
    }
}

impl blockstore::Blockstore for Blockstore {
    fn get<const S: usize>(
        &self,
        cid: &CidGeneric<S>,
    ) -> impl Future<Output = blockstore::Result<Option<Vec<u8>>>> + CondSend {
        let db = self.db.clone_boxed();
        let hash = *cid.hash();
        let codec = cid.codec();
        task::spawn_blocking(move || {
            let hash = Self::convert_multihash(&hash)?;
            match codec {
                Self::RAW_CODEC => {
                    let data = db.read_by_hash(hash);

                    if let Some(data) = &data
                        && data.len() as u64 > Self::MAX_BLOCK_SIZE
                    {
                        log::warn!("{hash} is too large: {} bytes", data.len());
                        return Err(blockstore::Error::ValueTooLarge);
                    }

                    Ok(data)
                }
                Self::ANNOUNCES_CODEC => {
                    let hash = unsafe { HashOf::new(hash) };
                    let announce = db.announce(hash);

                    if let Some(announce) = &announce
                        && announce.encoded_size() as u64 > Self::MAX_BLOCK_SIZE
                    {
                        log::warn!("{hash} is too large: {} bytes", announce.encoded_size());
                        return Err(blockstore::Error::ValueTooLarge);
                    }

                    Ok(announce.map(|announce| announce.encode()))
                }
                codec => Err(blockstore::Error::CidError(CidError::InvalidCidCodec(
                    codec,
                ))),
            }
        })
        .map(|res| res.expect("database panicked"))
    }

    async fn put_keyed<const S: usize>(
        &self,
        _cid: &CidGeneric<S>,
        _data: &[u8],
    ) -> blockstore::Result<()> {
        Ok(())
    }

    async fn remove<const S: usize>(&self, _cid: &CidGeneric<S>) -> blockstore::Result<()> {
        Ok(())
    }

    async fn close(self) -> blockstore::Result<()> {
        Ok(())
    }
}

struct Blake2b256Multihasher;

impl Multihasher<32> for Blake2b256Multihasher {
    async fn hash(
        &self,
        multihash_code: u64,
        input: &[u8],
    ) -> Result<Multihash<32>, MultihasherError> {
        if multihash_code != Blockstore::BLAKE2B_CODE {
            return Err(MultihasherError::UnknownMultihashCode);
        }

        let hash = ethexe_db::hash(input);
        let hash = Multihash::wrap(Blockstore::BLAKE2B_CODE, hash.as_bytes())
            .expect("size is always correct");
        Ok(hash)
    }
}

type InnerBehaviour = beetswap::Behaviour<32, Blockstore>;

pub struct Behaviour {
    inner: InnerBehaviour,
    handle: Handle,
    rx: mpsc::UnboundedReceiver<(Request, oneshot::Sender<Response>)>,
    requests: HashMap<beetswap::QueryId, (Request, oneshot::Sender<Response>)>,
}

impl Behaviour {
    pub fn new(db: Box<dyn BlockstoreDatabase>) -> Self {
        let (handle, rx) = mpsc::unbounded_channel();
        let blockstore = Arc::new(Blockstore { db });

        Self {
            inner: InnerBehaviour::builder(blockstore)
                .register_multihasher(Blake2b256Multihasher)
                .protocol_prefix("/ethexe")
                .expect("prefix is always correct")
                .build(),
            handle: Handle(handle),
            rx,
            requests: HashMap::new(),
        }
    }

    pub fn handle(&self) -> Handle {
        self.handle.clone()
    }

    fn handle_inner_event(&mut self, event: beetswap::Event) {
        match event {
            beetswap::Event::GetQueryResponse { query_id, data } => {
                if let Some((request, channel)) = self.requests.remove(&query_id) {
                    let response = request.into_response(data);
                    let _ = channel.send(response);
                }
            }
            beetswap::Event::GetQueryError { query_id, error } => {
                // The wrapper builds CIDs itself, so invalid multihashes are impossible.
                // Blockstore errors here mean local storage violated its read contract.
                panic!("{query_id:?} query failed: {error}");
            }
        }
    }
}

impl NetworkBehaviour for Behaviour {
    type ConnectionHandler = THandler<InnerBehaviour>;
    type ToSwarm = ();

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
        self.inner.on_swarm_event(event);
    }

    fn on_connection_handler_event(
        &mut self,
        peer_id: PeerId,
        connection_id: ConnectionId,
        event: THandlerOutEvent<Self>,
    ) {
        self.inner
            .on_connection_handler_event(peer_id, connection_id, event);
    }

    fn poll(
        &mut self,
        cx: &mut Context<'_>,
    ) -> Poll<ToSwarm<Self::ToSwarm, THandlerInEvent<Self>>> {
        self.requests.retain(|&query_id, (_, channel)| {
            if channel.is_closed() {
                self.inner.cancel(query_id);
                return false;
            }

            true
        });

        while let Poll::Ready(Some((request, channel))) = self.rx.poll_recv(cx) {
            let cid = request.into_cid();
            let query_id = self.inner.get(&cid);
            self.requests.insert(query_id, (request, channel));
        }

        if let Poll::Ready(to_swarm) = self.inner.poll(cx) {
            return match to_swarm {
                ToSwarm::GenerateEvent(event) => {
                    self.handle_inner_event(event);
                    Poll::Pending
                }
                to_swarm => {
                    Poll::Ready(to_swarm.map_out(|_event| {
                        unreachable!("`ToSwarm::GenerateEvent` is handled above")
                    }))
                }
            };
        }

        Poll::Pending
    }
}
