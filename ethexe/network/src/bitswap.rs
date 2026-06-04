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
use ethexe_common::db::HashStorageRO;
use futures::{FutureExt, Stream, stream::FuturesUnordered};
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
use std::{
    collections::HashMap,
    mem,
    sync::Arc,
    task::{Context, Poll},
    time::Duration,
};
use tokio::{
    sync::{mpsc, oneshot},
    task, time,
};

const BLAKE2B_CODE: u64 = 0xb220; // standard BLAKE2b multihash code
const RAW_CODEC: u64 = 0x55; // standard CID raw codec

#[derive(Debug, Clone, Copy, Default)]
pub struct Config {
    /// Restart stalled requests after some time.
    ///
    /// This is intended for test environment only, where a peer
    /// can receive an announce request before it has caught up enough to serve
    /// the corresponding data. Production environment is expected to have
    /// enough peers to fulfill requests, so request scheduling is left to
    /// Bitswap itself.
    pub auto_retry: bool,
}

impl Config {
    pub fn with_auto_retry(mut self, auto_retry: bool) -> Self {
        self.auto_retry = auto_retry;
        self
    }
}

#[derive(Clone)]
pub struct Handle {
    inner: mpsc::UnboundedSender<(H256, oneshot::Sender<Vec<u8>>)>,
    auto_retry: bool,
}

impl Handle {
    const RETRY_TIMEOUT: Duration = Duration::from_secs(5);

    pub async fn request(&self, request: H256) -> Vec<u8> {
        if !self.auto_retry {
            return self.inner_request(request).await;
        }

        loop {
            match time::timeout(Self::RETRY_TIMEOUT, self.inner_request(request)).await {
                Ok(response) => return response,
                Err(_) => {
                    log::warn!("Bitswap request {request:?} timed out, retrying");
                }
            }
        }
    }

    pub fn request_many<'a>(
        &'a self,
        iter: impl Iterator<Item = H256> + 'a,
    ) -> impl Stream<Item = (H256, Vec<u8>)> + 'a {
        FuturesUnordered::from_iter(
            iter.map(|hash| self.request(hash).map(move |data| (hash, data))),
        )
    }

    async fn inner_request(&self, request: H256) -> Vec<u8> {
        let (tx, rx) = oneshot::channel();

        self.inner
            .send((request, tx))
            .expect("channel should never be closed");

        rx.await.expect("channel should never be closed")
    }
}

pub(crate) trait BlockstoreDatabase: Send + Sync + HashStorageRO {
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

    fn convert_multihash<const S: usize>(multihash: &Multihash<S>) -> blockstore::Result<H256> {
        let hash: Multihash<32> =
            beetswap::utils::convert_multihash(multihash).ok_or(blockstore::Error::CidTooLarge)?;
        if hash.code() != BLAKE2B_CODE {
            return Err(blockstore::Error::CidError(CidError::InvalidMultihashCode(
                hash.code(),
                BLAKE2B_CODE,
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
            let data = match codec {
                RAW_CODEC => {
                    let data = db.read_by_hash(hash);

                    if let Some(data) = &data
                        && data.len() as u64 > Self::MAX_BLOCK_SIZE
                    {
                        log::warn!("{hash} is too large: {} bytes", data.len());
                        return Err(blockstore::Error::ValueTooLarge);
                    }

                    data
                }
                codec => Err(blockstore::Error::CidError(CidError::InvalidCidCodec(
                    codec,
                )))?,
            };

            if let Some(data) = &data
                && data.len() as u64 > Self::MAX_BLOCK_SIZE
            {
                log::warn!("{hash} is too large: {} bytes", data.len());
                return Err(blockstore::Error::ValueTooLarge);
            }

            Ok(data)
        })
        .map(|res| {
            res.map_err(|err| blockstore::Error::FatalDatabaseError(err.to_string()))
                .flatten()
        })
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
        if multihash_code != BLAKE2B_CODE {
            return Err(MultihasherError::UnknownMultihashCode);
        }

        let hash = ethexe_db::hash(input);
        let hash = Multihash::wrap(BLAKE2B_CODE, hash.as_bytes()).expect("size is always correct");
        Ok(hash)
    }
}

type InnerBehaviour = beetswap::Behaviour<32, Blockstore>;

pub struct Behaviour {
    inner: InnerBehaviour,
    handle: Handle,
    rx: mpsc::UnboundedReceiver<(H256, oneshot::Sender<Vec<u8>>)>,
    requests: HashMap<beetswap::QueryId, oneshot::Sender<Vec<u8>>>,
}

impl Behaviour {
    pub fn new(db: Box<dyn BlockstoreDatabase>, config: Config) -> Self {
        let (handle, rx) = mpsc::unbounded_channel();
        let blockstore = Arc::new(Blockstore { db });

        Self {
            inner: InnerBehaviour::builder(blockstore)
                .register_multihasher(Blake2b256Multihasher)
                .protocol_prefix("/ethexe")
                .expect("prefix is always correct")
                .build(),
            handle: Handle {
                inner: handle,
                auto_retry: config.auto_retry,
            },
            rx,
            requests: HashMap::new(),
        }
    }

    pub fn handle(&self) -> Handle {
        self.handle.clone()
    }

    fn create_cid(hash: H256) -> Cid {
        Cid::new_v1(
            RAW_CODEC,
            Multihash::wrap(BLAKE2B_CODE, hash.as_bytes()).expect("size is always correct"),
        )
    }

    fn handle_inner_event(&mut self, event: beetswap::Event) {
        match event {
            beetswap::Event::GetQueryResponse { query_id, data } => {
                if let Some(channel) = self.requests.remove(&query_id) {
                    let _ = channel.send(data);
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
        self.requests.retain(|&query_id, channel| {
            if channel.is_closed() {
                self.inner.cancel(query_id);
                return false;
            }

            true
        });

        while let Poll::Ready(Some((request, channel))) = self.rx.poll_recv(cx) {
            let cid = Self::create_cid(request);
            let query_id = self.inner.get(&cid);
            self.requests.insert(query_id, channel);
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

#[cfg(test)]
mod tests {
    use super::*;
    use assert_matches::assert_matches;
    use blockstore::Blockstore as _;

    #[derive(Clone)]
    struct PanickingDatabase;

    impl HashStorageRO for PanickingDatabase {
        fn read_by_hash(&self, _hash: H256) -> Option<Vec<u8>> {
            panic!("database read panic");
        }
    }

    impl BlockstoreDatabase for PanickingDatabase {
        fn clone_boxed(&self) -> Box<dyn BlockstoreDatabase> {
            Box::new(self.clone())
        }
    }

    #[tokio::test]
    async fn blockstore_reads_raw_data() {
        let db = ethexe_db::Database::memory();
        let hash = db.cas().write(b"hello");
        let blockstore = Blockstore { db: Box::new(db) };
        let cid = Behaviour::create_cid(hash);

        let data = blockstore.get(&cid).await.unwrap();

        assert_eq!(data, Some(b"hello".to_vec()));
    }

    #[tokio::test]
    async fn blockstore_rejects_unknown_codec() {
        let db = ethexe_db::Database::memory();
        let blockstore = Blockstore { db: Box::new(db) };
        let hash = H256::from([3; 32]);
        let multihash = Multihash::wrap(BLAKE2B_CODE, hash.as_bytes()).unwrap();
        let cid = Cid::new_v1(0x99, multihash);

        let error = blockstore.get(&cid).await.unwrap_err();

        assert_matches!(
            error,
            blockstore::Error::CidError(CidError::InvalidCidCodec(0x99))
        );
    }

    #[tokio::test]
    async fn blockstore_rejects_unknown_multihash_code() {
        let db = ethexe_db::Database::memory();
        let blockstore = Blockstore { db: Box::new(db) };
        let hash = H256::from([4; 32]);
        let multihash = Multihash::wrap(0x12, hash.as_bytes()).unwrap();
        let cid = Cid::new_v1(RAW_CODEC, multihash);

        let error = blockstore.get(&cid).await.unwrap_err();

        assert_matches!(
            error,
            blockstore::Error::CidError(CidError::InvalidMultihashCode(0x12, BLAKE2B_CODE))
        );
    }

    #[tokio::test]
    async fn blockstore_rejects_oversized_raw_data() {
        let db = ethexe_db::Database::memory();
        let hash = db
            .cas()
            .write(&vec![0; Blockstore::MAX_BLOCK_SIZE as usize + 1]);
        let blockstore = Blockstore { db: Box::new(db) };
        let cid = Behaviour::create_cid(hash);

        let error = blockstore.get(&cid).await.unwrap_err();

        assert_matches!(error, blockstore::Error::ValueTooLarge);
    }

    #[tokio::test]
    async fn blockstore_maps_database_panic_to_fatal_database_error() {
        let blockstore = Blockstore {
            db: Box::new(PanickingDatabase),
        };
        let cid = Behaviour::create_cid(H256::from([6; 32]));

        let error = blockstore.get(&cid).await.unwrap_err();

        assert_matches!(
            error,
            blockstore::Error::FatalDatabaseError(message)
                if message.contains("database read panic")
        );
    }

    #[tokio::test]
    async fn blake2b_multihasher_hashes_known_code() {
        let multihash = Blake2b256Multihasher
            .hash(BLAKE2B_CODE, b"hello")
            .await
            .unwrap();

        assert_eq!(multihash.code(), BLAKE2B_CODE);
        assert_eq!(multihash.digest(), ethexe_db::hash(b"hello").as_bytes());
    }

    #[tokio::test]
    async fn blake2b_multihasher_rejects_unknown_code() {
        let error = Blake2b256Multihasher
            .hash(0x12, b"hello")
            .await
            .unwrap_err();

        assert_matches!(error, MultihasherError::UnknownMultihashCode);
    }

    #[tokio::test(start_paused = true)]
    async fn handle_retries_timed_out_requests() {
        let (inner, mut rx) = mpsc::unbounded_channel();
        let handle = Handle {
            inner,
            auto_retry: true,
        };
        let hash = H256::from([5; 32]);

        let pending = tokio::spawn(async move { handle.request(hash).await });

        let (request, first_response) = rx.recv().await.unwrap();
        assert_eq!(request, hash);

        time::advance(Handle::RETRY_TIMEOUT).await;
        let (request, second_response) = rx.recv().await.unwrap();
        assert_eq!(request, hash);
        assert!(first_response.is_closed());

        second_response.send(b"hello".to_vec()).unwrap();
        let response = pending.await.unwrap();

        assert_eq!(response, b"hello".to_vec());
    }
}
