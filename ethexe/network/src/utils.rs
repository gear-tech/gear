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
use ip_network::IpNetwork;
use libp2p::{
    Multiaddr, StreamProtocol,
    futures::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt},
    multiaddr, request_response,
    swarm::{
        ConnectionClosed, ConnectionId, DialError, DialFailure, FromSwarm, NewExternalAddrOfPeer,
        behaviour::ConnectionEstablished,
    },
};
use lru::LruCache;
use parity_scale_codec::{Decode, DecodeAll, Encode};
use std::{
    collections::{BTreeMap, BTreeSet, HashMap, HashSet, hash_map::Entry},
    convert::Infallible,
    fmt, io,
    marker::PhantomData,
    num::NonZeroUsize,
    pin::Pin,
    task::{Context, Poll, ready},
    time::Duration,
};
use tokio::{time, time::Instant};

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

pub(crate) trait MultiaddrExt {
    fn is_global(&self) -> bool;
}

impl MultiaddrExt for Multiaddr {
    fn is_global(&self) -> bool {
        // we use `ip_network` crate instead of std method because it's unstable
        self.iter().all(|protocol| match protocol {
            multiaddr::Protocol::Ip4(ip) => IpNetwork::from(ip).is_global(),
            multiaddr::Protocol::Ip6(ip) => IpNetwork::from(ip).is_global(),
            _ => true,
        })
    }
}

pub(crate) trait ConnectionMapLimit {
    type Error;

    fn check_limit(
        &self,
        connections: &HashMap<PeerId, HashSet<ConnectionId>>,
        peer_id: PeerId,
    ) -> Result<(), Self::Error>;
}

#[derive(Debug)]
pub(crate) struct ConnectionLimitError {
    pub limit: u32,
}

pub(crate) struct ConnectionLimit {
    limit: u32,
}

impl ConnectionMapLimit for ConnectionLimit {
    type Error = ConnectionLimitError;

    fn check_limit(
        &self,
        connections: &HashMap<PeerId, HashSet<ConnectionId>>,
        peer_id: PeerId,
    ) -> Result<(), Self::Error> {
        let current = connections
            .get(&peer_id)
            .map(|connections| connections.len())
            .unwrap_or(0) as u32;
        if current < self.limit {
            Ok(())
        } else {
            Err(ConnectionLimitError { limit: self.limit })
        }
    }
}

pub(crate) struct NoLimits;

impl ConnectionMapLimit for NoLimits {
    type Error = Infallible;

    fn check_limit(
        &self,
        _connections: &HashMap<PeerId, HashSet<ConnectionId>>,
        _peer_id: PeerId,
    ) -> Result<(), Self::Error> {
        Ok(())
    }
}

#[derive(Debug, Default, Clone)]
pub(crate) struct ConnectionMap<T> {
    inner: HashMap<PeerId, HashSet<ConnectionId>>,
    limit: T,
}

impl<T: ConnectionMapLimit> ConnectionMap<T> {
    pub fn contains_peer(&self, peer_id: &PeerId) -> bool {
        self.inner.contains_key(peer_id)
    }

    pub fn peers(&self) -> impl ExactSizeIterator<Item = PeerId> {
        self.inner.keys().copied()
    }

    pub(crate) fn add_connection(
        &mut self,
        peer_id: PeerId,
        connection_id: ConnectionId,
    ) -> Result<bool, T::Error> {
        self.limit.check_limit(&self.inner, peer_id)?;
        let new = self.inner.entry(peer_id).or_default().insert(connection_id);
        Ok(new)
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

impl ConnectionMap<ConnectionLimit> {
    pub(crate) fn with_connection_limit(limit: u32) -> Self {
        Self {
            inner: Default::default(),
            limit: ConnectionLimit { limit },
        }
    }
}

impl ConnectionMap<NoLimits> {
    pub(crate) fn without_limits() -> Self {
        Self {
            inner: Default::default(),
            limit: NoLimits,
        }
    }

    /// Returns true if a new connection added
    pub(crate) fn on_swarm_event(&mut self, event: FromSwarm) -> bool {
        match event {
            FromSwarm::ConnectionEstablished(ConnectionEstablished {
                peer_id,
                connection_id,
                ..
            }) => {
                let Ok(new) = self.add_connection(peer_id, connection_id);
                new
            }
            FromSwarm::ConnectionClosed(ConnectionClosed {
                peer_id,
                connection_id,
                ..
            }) => {
                self.remove_connection(peer_id, connection_id);
                false
            }
            _ => false,
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

#[derive(Debug)]
pub struct ExponentialBackoffInterval {
    delay: Pin<Box<time::Sleep>>,
    next_duration: Duration,
    max: Duration,
}

impl ExponentialBackoffInterval {
    pub const FACTOR: u32 = 2;

    pub fn new(start: Duration, max: Duration) -> Self {
        Self {
            delay: Box::pin(time::sleep(start)),
            next_duration: start,
            max,
        }
    }

    fn reset(&mut self, new_duration: Duration) {
        self.next_duration = new_duration;
        self.delay
            .as_mut()
            .reset(Instant::now() + self.next_duration);
    }

    #[cfg(test)]
    pub fn period(&self) -> Duration {
        self.next_duration
    }

    pub fn tick_at_max(&mut self) {
        self.reset(self.max);
    }

    pub fn poll_tick(&mut self, cx: &mut Context) -> Poll<()> {
        ready!(self.delay.as_mut().poll(cx));

        let new_duration = (self.next_duration * Self::FACTOR).min(self.max);
        self.reset(new_duration);

        Poll::Ready(())
    }
}

/// Literally [`libp2p::swarm::PeerAddresses`], but we can iterate over peers
#[derive(Debug)]
pub struct PeerAddresses(LruCache<PeerId, LruCache<Multiaddr, ()>>);

impl PeerAddresses {
    fn new(number_of_peers: NonZeroUsize) -> Self {
        Self(LruCache::new(number_of_peers))
    }

    fn prepare_addr(peer: &PeerId, addr: &Multiaddr) -> Result<Multiaddr, Multiaddr> {
        addr.clone().with_p2p(*peer)
    }

    pub fn on_swarm_event(&mut self, event: &FromSwarm) -> bool {
        match event {
            FromSwarm::NewExternalAddrOfPeer(NewExternalAddrOfPeer { peer_id, addr }) => {
                self.add(*peer_id, (*addr).clone())
            }
            FromSwarm::DialFailure(DialFailure {
                peer_id: Some(peer_id),
                error: DialError::Transport(errors),
                ..
            }) => {
                for (addr, _error) in errors {
                    self.remove(peer_id, addr);
                }
                true
            }
            _ => false,
        }
    }

    pub fn add(&mut self, peer: PeerId, address: Multiaddr) -> bool {
        match Self::prepare_addr(&peer, &address) {
            Ok(address) => {
                if let Some(cached) = self.0.get_mut(&peer) {
                    cached.put(address, ()).is_none()
                } else {
                    let mut set = LruCache::new(NonZeroUsize::new(10).expect("10 > 0"));
                    set.put(address, ());
                    self.0.put(peer, set);

                    true
                }
            }
            Err(_) => false,
        }
    }

    pub fn remove(&mut self, peer: &PeerId, address: &Multiaddr) -> bool {
        match self.0.get_mut(peer) {
            Some(addrs) => match Self::prepare_addr(peer, address) {
                Ok(address) => addrs.pop(&address).is_some(),
                Err(_) => false,
            },
            None => false,
        }
    }

    pub fn iter(&self) -> impl Iterator<Item = (&PeerId, impl Iterator<Item = &Multiaddr>)> {
        self.0
            .iter()
            .map(|(peer, set)| (peer, set.iter().map(|(addr, ())| addr)))
    }
}

impl Default for PeerAddresses {
    fn default() -> Self {
        Self::new(NonZeroUsize::new(100).unwrap())
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use crate::{
        db_sync::PeerId,
        utils::{ConnectionMap, ExponentialBackoffInterval},
    };
    use libp2p::swarm::ConnectionId;
    use std::{collections::HashSet, future, time::Duration};
    use tokio::time;
    use tracing_subscriber::EnvFilter;

    const TIMER_START: Duration = Duration::from_secs(1);
    const TIMER_MAX: Duration = Duration::from_secs(60);

    pub fn init_logger() {
        let _ = tracing_subscriber::fmt()
            .with_env_filter(EnvFilter::from_default_env())
            .with_test_writer()
            .try_init();
    }

    #[test]
    fn connection_map_limit_works() {
        const LIMIT: u32 = 5;

        let mut map = ConnectionMap::with_connection_limit(LIMIT);

        let main_peer = PeerId::random();

        for i in 0..LIMIT {
            map.add_connection(main_peer, ConnectionId::new_unchecked(i as usize))
                .unwrap();
        }

        let limit = map
            .add_connection(main_peer, ConnectionId::new_unchecked(usize::MAX))
            .unwrap_err()
            .limit;
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
        let mut map = ConnectionMap::without_limits();

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

    #[tokio::test(start_paused = true)]
    async fn interval_smoke() {
        let mut interval = ExponentialBackoffInterval::new(TIMER_START, TIMER_MAX);
        assert_eq!(
            interval.next_duration,
            TIMER_START * ExponentialBackoffInterval::FACTOR.pow(0)
        );

        future::poll_fn(|cx| interval.poll_tick(cx)).await;
        assert_eq!(
            interval.next_duration,
            TIMER_START * ExponentialBackoffInterval::FACTOR.pow(1)
        );

        future::poll_fn(|cx| interval.poll_tick(cx)).await;
        assert_eq!(
            interval.next_duration,
            TIMER_START * ExponentialBackoffInterval::FACTOR.pow(2)
        );

        while interval.next_duration != TIMER_MAX {
            future::poll_fn(|cx| interval.poll_tick(cx)).await;
        }

        assert_eq!(interval.next_duration, TIMER_MAX);
        assert_eq!(interval.next_duration, TIMER_MAX);
        assert_eq!(interval.next_duration, TIMER_MAX);
    }

    #[tokio::test(start_paused = true)]
    async fn interval_tick_at_max() {
        let mut interval = ExponentialBackoffInterval::new(TIMER_START, TIMER_MAX);
        interval.tick_at_max();

        let instant = time::Instant::now();

        future::poll_fn(|cx| interval.poll_tick(cx)).await;
        assert_eq!(interval.next_duration, TIMER_MAX);
        assert_eq!(instant.elapsed(), TIMER_MAX);

        future::poll_fn(|cx| interval.poll_tick(cx)).await;
        assert_eq!(interval.next_duration, TIMER_MAX);
        assert_eq!(instant.elapsed(), TIMER_MAX * 2);
    }
}
