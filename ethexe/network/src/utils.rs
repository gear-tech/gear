// This file is part of Gear.
//
// Copyright (C) 2024-2025 Gear Technologies Inc.
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

use crate::db_sync::PeerId;
use async_trait::async_trait;
use libp2p::{
    futures::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt},
    request_response,
    swarm::ConnectionId,
    StreamProtocol,
};
use parity_scale_codec::{Decode, DecodeAll, Encode};
use std::{
    collections::{hash_map::Entry, BTreeMap, BTreeSet, HashMap, HashSet},
    fmt, io,
    marker::PhantomData,
};

pub struct ParityScaleCodec<Req, Resp>(PhantomData<(Req, Resp)>);

impl<Req, Resp> ParityScaleCodec<Req, Resp> {
    const MAX_REQUEST_SIZE: u64 = 1024 * 1024;
    const MAX_RESPONSE_SIZE: u64 = 10 * 1024 * 1024;
}

#[async_trait]
impl<Req, Resp> request_response::Codec for ParityScaleCodec<Req, Resp>
where
    Req: Send + Encode + Decode,
    Resp: Send + Encode + Decode,
{
    type Protocol = StreamProtocol;
    type Request = Req;
    type Response = Resp;

    async fn read_request<T>(
        &mut self,
        _protocol: &Self::Protocol,
        io: &mut T,
    ) -> io::Result<Self::Request>
    where
        T: AsyncRead + Unpin + Send,
    {
        let mut vec = Vec::new();
        io.take(Self::MAX_REQUEST_SIZE)
            .read_to_end(&mut vec)
            .await?;
        Req::decode_all(&mut vec.as_slice()).map_err(io::Error::other)
    }

    async fn read_response<T>(
        &mut self,
        _protocol: &Self::Protocol,
        io: &mut T,
    ) -> io::Result<Self::Response>
    where
        T: AsyncRead + Unpin + Send,
    {
        let mut vec = Vec::new();
        io.take(Self::MAX_RESPONSE_SIZE)
            .read_to_end(&mut vec)
            .await?;
        Resp::decode_all(&mut vec.as_slice()).map_err(io::Error::other)
    }

    async fn write_request<T>(
        &mut self,
        _protocol: &Self::Protocol,
        io: &mut T,
        req: Self::Request,
    ) -> io::Result<()>
    where
        T: AsyncWrite + Unpin + Send,
    {
        let vec = req.encode();
        io.write_all(&vec).await?;
        Ok(())
    }

    async fn write_response<T>(
        &mut self,
        _protocol: &Self::Protocol,
        io: &mut T,
        res: Self::Response,
    ) -> io::Result<()>
    where
        T: AsyncWrite + Unpin + Send,
    {
        let vec = res.encode();
        io.write_all(&vec).await?;
        Ok(())
    }
}

impl<Req, Resp> Default for ParityScaleCodec<Req, Resp> {
    fn default() -> Self {
        Self(PhantomData)
    }
}

impl<Req, Resp> Copy for ParityScaleCodec<Req, Resp> {}

impl<Req, Resp> Clone for ParityScaleCodec<Req, Resp> {
    fn clone(&self) -> Self {
        *self
    }
}

#[derive(Debug, Default, Clone)]
pub(crate) struct ConnectionMap {
    inner: HashMap<PeerId, HashSet<ConnectionId>>,
    limit: Option<u32>,
}

impl ConnectionMap {
    pub(crate) fn new(limit: Option<u32>) -> Self {
        Self {
            inner: Default::default(),
            limit,
        }
    }

    fn check_limit(&self, peer_id: PeerId) -> Result<(), u32> {
        let current = self
            .inner
            .get(&peer_id)
            .map(|connections| connections.len())
            .unwrap_or(0) as u32;
        let limit = self.limit.unwrap_or(u32::MAX);
        if current < limit {
            Ok(())
        } else {
            Err(limit)
        }
    }

    pub fn peers(&self) -> impl Iterator<Item = PeerId> {
        self.inner.keys().copied()
    }

    pub(crate) fn add_connection(
        &mut self,
        peer_id: PeerId,
        connection_id: ConnectionId,
    ) -> Result<(), u32> {
        self.check_limit(peer_id)?;
        self.inner.entry(peer_id).or_default().insert(connection_id);
        Ok(())
    }

    pub(crate) fn remove_connection(&mut self, peer_id: PeerId, connection_id: ConnectionId) {
        if let Entry::Occupied(mut entry) = self.inner.entry(peer_id) {
            let connections = entry.get_mut();
            connections.remove(&connection_id);

            if connections.is_empty() {
                entry.remove();
            }
        }
    }
}

/// A helper struct for formatting collections (BTreeSet, BTreeMap) with two display modes:
/// - alternate mode (`{:#?}`) - shows full collection contents
/// - normal mode (`{:?}`) - shows only collection length and item type description
#[allow(dead_code)] // clippy fails to detect it's actually used
pub(crate) struct AlternateCollectionFmt<T> {
    collection: T,
    len: usize,
    items: &'static str,
}

impl<'a, T> AlternateCollectionFmt<&'a BTreeSet<T>> {
    #[allow(dead_code)]
    pub fn set(collection: &'a BTreeSet<T>, items: &'static str) -> Self {
        Self {
            len: collection.len(),
            collection,
            items,
        }
    }
}

impl<'a, K, V> AlternateCollectionFmt<&'a BTreeMap<K, V>> {
    #[allow(dead_code)]
    pub fn map(collection: &'a BTreeMap<K, V>, items: &'static str) -> Self {
        Self {
            len: collection.len(),
            collection,
            items,
        }
    }
}

impl<T: fmt::Debug> fmt::Debug for AlternateCollectionFmt<T> {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        if f.alternate() {
            self.collection.fmt(f)
        } else {
            f.write_fmt(format_args!(
                "{len} {items}",
                len = self.len,
                items = self.items
            ))
        }
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use crate::{db_sync::PeerId, utils::ConnectionMap};
    use libp2p::swarm::ConnectionId;
    use std::collections::HashSet;
    use tracing_subscriber::EnvFilter;

    pub fn init_logger() {
        let _ = tracing_subscriber::fmt()
            .with_env_filter(EnvFilter::from_default_env())
            .with_test_writer()
            .try_init();
    }

    #[test]
    fn connection_map_limit_works() {
        const LIMIT: u32 = 5;

        let mut map = ConnectionMap::new(Some(LIMIT));

        let main_peer = PeerId::random();

        for i in 0..LIMIT {
            map.add_connection(main_peer, ConnectionId::new_unchecked(i as usize))
                .unwrap();
        }

        let limit = map
            .add_connection(main_peer, ConnectionId::new_unchecked(usize::MAX))
            .unwrap_err();
        assert_eq!(limit, LIMIT);

        // new peer so no limit exceeded yet
        map.add_connection(
            PeerId::random(),
            ConnectionId::new_unchecked(usize::MAX / 2),
        )
        .unwrap();
    }

    #[test]
    fn connection_map_key_cleared() {
        let mut map = ConnectionMap::new(None);

        let peer_set: HashSet<PeerId> = [
            PeerId::random(),
            PeerId::random(),
            PeerId::random(),
            PeerId::random(),
            PeerId::random(),
        ]
        .into();
        let new_connection_id = |i, j| ConnectionId::new_unchecked(i * (j as usize + 10));

        for (i, &peer) in peer_set.iter().enumerate() {
            for j in 0..10 {
                map.add_connection(peer, new_connection_id(i, j)).unwrap();
            }
        }

        assert_eq!(
            map.inner.clone().into_keys().collect::<HashSet<PeerId>>(),
            peer_set
        );

        for (i, &peer) in peer_set.iter().enumerate() {
            for j in 0..10 {
                map.remove_connection(peer, new_connection_id(i, j));
            }
        }

        assert_eq!(
            map.inner.into_keys().collect::<HashSet<PeerId>>(),
            HashSet::default()
        );
    }
}
