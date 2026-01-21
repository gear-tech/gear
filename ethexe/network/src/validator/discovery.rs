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

//! Validator discovery
//!
//! Heavily based on Substrate authority discovery mechanism.

use crate::{
    db_sync::PeerId,
    kad::{
        self, GetRecordResult, PutRecordFuture, RecordKey, ValidatorIdentityKey,
        ValidatorIdentityRecord,
    },
    utils::ExponentialBackoffInterval,
    validator::list::ValidatorListSnapshot,
};
use anyhow::Context as _;
use ethexe_common::{
    Address, ToDigest,
    ecdsa::{PublicKey, Signature},
    sha3::Keccak256,
};
use futures::{
    FutureExt, StreamExt,
    stream::{self, BoxStream},
};
use gsigner::secp256k1::{PrivateKey, Secp256k1SignerExt, Signer};
use indexmap::IndexSet;
use libp2p::{
    Multiaddr,
    core::{Endpoint, transport::PortUse},
    identity::Keypair,
    multiaddr,
    swarm::{
        ConnectionDenied, ConnectionId, ExternalAddresses, FromSwarm, NetworkBehaviour, THandler,
        THandlerInEvent, THandlerOutEvent, ToSwarm, dummy,
    },
};
use parity_scale_codec::{Decode, Encode, Input, Output};
use std::{
    collections::{HashMap, VecDeque},
    sync::Arc,
    task::{Context, Poll, ready},
    time::SystemTime,
};

/// From Substrate sources:
/// Maximum number of addresses cached per authority. Additional addresses are discarded.
const MAX_IDENTITY_ADDRESSES: usize = 10;
/// Number of concurrent queries to get validator identity.
///
/// Limit is to not flood the network
const MAX_IN_FLIGHT_QUERIES: usize = 10;

pub type ValidatorIdentities = HashMap<Address, SignedValidatorIdentity>;

/// Signed validator discovery
///
/// Signed by both validator key and networking key,
/// so validator cannot provide network addresses from other peer
/// and network peer cannot claim it is a validator.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SignedValidatorIdentity {
    inner: ValidatorIdentity,
    validator_signature: Signature,
    validator_key: PublicKey,
    network_signature: Signature,
    network_key: libp2p::identity::secp256k1::PublicKey,
}

impl SignedValidatorIdentity {
    pub(crate) fn addresses(&self) -> &ValidatorAddresses {
        &self.inner.addresses
    }

    pub(crate) fn address(&self) -> Address {
        self.validator_key.to_address()
    }

    pub(crate) fn peer_id(&self) -> PeerId {
        libp2p::identity::PublicKey::from(self.network_key.clone()).to_peer_id()
    }
}

impl Encode for SignedValidatorIdentity {
    fn encode_to<T: Output + ?Sized>(&self, dest: &mut T) {
        let Self {
            inner,
            validator_signature,
            validator_key: _,
            network_signature,
            network_key: _,
        } = self;

        inner.encode_to(dest);
        validator_signature.encode_to(dest);
        network_signature.encode_to(dest);
    }
}

impl Decode for SignedValidatorIdentity {
    fn decode<I: Input>(input: &mut I) -> Result<Self, parity_scale_codec::Error> {
        let inner = ValidatorIdentity::decode(input)?;

        let validator_signature = Signature::decode(input)?;
        let validator_key = validator_signature.validate(&inner).map_err(|err| {
            parity_scale_codec::Error::from("failed to validate signature").chain(err.to_string())
        })?;

        let network_signature = Signature::decode(input)?;
        let network_key = network_signature.validate(&inner).map_err(|err| {
            parity_scale_codec::Error::from("failed to validate network signature")
                .chain(err.to_string())
        })?;
        let network_key =
            libp2p::identity::secp256k1::PublicKey::try_from_bytes(&network_key.to_bytes())
                .expect("we use secp256k1 for networking key");

        let this = Self {
            inner,
            validator_signature,
            validator_key,
            network_signature,
            network_key,
        };

        if this.peer_id() != this.inner.addresses.peer_id() {
            return Err(parity_scale_codec::Error::from(
                "addresses peer ID differs from signature peer ID",
            ));
        }

        Ok(this)
    }
}

#[derive(Debug, derive_more::Display)]
enum FromVecOfVecError {
    #[display("no addresses")]
    NoAddresses,
    #[display("multiaddr parse error: {_0}")]
    ParseMultiaddr(multiaddr::Error),
    #[display("multiaddr is empty")]
    MultiaddrIsEmpty,
    #[display("peer ID mismatch: expected={expected}, actual={actual}")]
    PeerIdMismatch {
        expected: Box<PeerId>,
        actual: Box<PeerId>,
    },
    #[display("last protocol is not P2P")]
    LastProtocolIsNotP2p,
    #[display("duplicated multiaddr")]
    DuplicatedMultiaddr,
    #[display("too many addresses: expected={expected}, actual={actual}")]
    TooManyAddresses { expected: usize, actual: usize },
}

/// Validator addresses
///
/// Contains at least 1 address.
/// Every address ends with P2P protocol containing the same peer ID.
///
/// Duplicated addresses are denied during decoding.
// TODO: consider to not expect peer ID at the end of addresses
// because it is signed by the network key (and thus the same peer ID)
#[derive(Debug, Clone, PartialEq, Eq, derive_more::IntoIterator)]
#[into_iterator(owned, ref)]
pub(crate) struct ValidatorAddresses {
    // use indexed set for stable encoding/decoding and digest generation
    addresses: IndexSet<Multiaddr>,
}

impl ValidatorAddresses {
    #[cfg(test)]
    pub(crate) fn new(peer_id: PeerId, address: Multiaddr) -> Self {
        Self {
            addresses: [address
                .with_p2p(peer_id)
                .expect("peer ID should be the same")]
            .into(),
        }
    }

    fn from_external_addresses(
        peer_id: PeerId,
        external_addresses: &ExternalAddresses,
    ) -> Option<Self> {
        let addresses = external_addresses.as_slice();
        if addresses.is_empty() {
            return None;
        }

        let addresses: IndexSet<Multiaddr> = addresses
            .iter()
            .take(MAX_IDENTITY_ADDRESSES)
            .cloned()
            .map(|address| {
                address
                    .with_p2p(peer_id)
                    .expect("peer ID should be the same")
            })
            .collect();

        Some(Self { addresses })
    }

    /// Constructor to be used in `Decode` implementation
    fn from_vec_of_vec(addresses: Vec<Vec<u8>>) -> Result<Self, FromVecOfVecError> {
        if addresses.is_empty() {
            return Err(FromVecOfVecError::NoAddresses);
        }

        if addresses.len() > MAX_IDENTITY_ADDRESSES {
            return Err(FromVecOfVecError::TooManyAddresses {
                expected: MAX_IDENTITY_ADDRESSES,
                actual: addresses.len(),
            });
        }

        let (addresses, _peer_id) = addresses.into_iter().try_fold(
            (IndexSet::new(), None),
            |(mut set, mut peer_id), addr| {
                let addr = Multiaddr::try_from(addr).map_err(FromVecOfVecError::ParseMultiaddr)?;

                let protocol = addr
                    .iter()
                    .last()
                    .ok_or(FromVecOfVecError::MultiaddrIsEmpty)?;
                if let multiaddr::Protocol::P2p(address_peer_id) = protocol {
                    let peer_id = *peer_id.get_or_insert(address_peer_id);
                    if peer_id != address_peer_id {
                        return Err(FromVecOfVecError::PeerIdMismatch {
                            expected: Box::new(peer_id),
                            actual: Box::new(address_peer_id),
                        });
                    }
                } else {
                    return Err(FromVecOfVecError::LastProtocolIsNotP2p);
                }

                if !set.insert(addr) {
                    return Err(FromVecOfVecError::DuplicatedMultiaddr);
                }

                Ok((set, peer_id))
            },
        )?;

        Ok(Self { addresses })
    }

    fn peer_id(&self) -> PeerId {
        if let multiaddr::Protocol::P2p(peer_id) = self
            .addresses
            .iter()
            .next()
            .expect("always contains at least one address")
            .iter()
            .last()
            .expect("always contains at least one protocol")
        {
            peer_id
        } else {
            unreachable!("always contains `p2p` protocol as last")
        }
    }

    pub(crate) fn iter(&self) -> impl Iterator<Item = &Multiaddr> {
        self.addresses.iter()
    }
}

impl Encode for ValidatorAddresses {
    fn encode_to<T: Output + ?Sized>(&self, dest: &mut T) {
        let addresses: Vec<_> = self.addresses.iter().map(|addr| addr.as_ref()).collect();
        addresses.encode_to(dest);
    }
}

impl Decode for ValidatorAddresses {
    fn decode<I: Input>(input: &mut I) -> Result<Self, parity_scale_codec::Error> {
        let addresses = <Vec<Vec<u8>>>::decode(input)?;
        let addresses = Self::from_vec_of_vec(addresses).map_err(|e| {
            parity_scale_codec::Error::from("failed to convert addresses").chain(e.to_string())
        })?;
        Ok(addresses)
    }
}

impl ToDigest for ValidatorAddresses {
    fn update_hasher(&self, hasher: &mut Keccak256) {
        for address in self.addresses.iter() {
            address.as_ref().update_hasher(hasher);
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub struct ValidatorIdentity {
    pub addresses: ValidatorAddresses,
    pub creation_time: u128,
}

impl ValidatorIdentity {
    pub(crate) fn sign(
        self,
        signer: &Signer,
        validator_key: PublicKey,
        keypair: &Keypair,
    ) -> anyhow::Result<SignedValidatorIdentity> {
        let digest = self.to_digest();
        let validator_signature = signer
            .sign_digest(validator_key, &digest)
            .context("failed to sign validator identity with validator key")?;

        let network_private_key = keypair
            .clone()
            .try_into_secp256k1()
            .expect("we use secp256k1 for networking key")
            .secret()
            .to_bytes();
        let network_private_key = PrivateKey::from_seed(network_private_key)
            .context("failed to construct network private key")?;
        let network_signature = Signature::create(&network_private_key, &self)
            .context("failed to sign validator identity with networking key")?;
        let network_key = keypair
            .public()
            .try_into_secp256k1()
            .expect("we use secp256k1 for networking key");

        Ok(SignedValidatorIdentity {
            inner: self,
            validator_signature,
            validator_key,
            network_signature,
            network_key,
        })
    }
}

impl ToDigest for ValidatorIdentity {
    fn update_hasher(&self, hasher: &mut Keccak256) {
        let Self {
            addresses,
            creation_time,
        } = self;

        addresses.update_hasher(hasher);
        creation_time.to_be_bytes().update_hasher(hasher);
    }
}

#[derive(Debug, Eq, PartialEq)]
pub enum Event {
    GetIdentitiesStarted,
    IdentityUpdated { address: Address },
    PutIdentityStarted,
    PutIdentityTicksAtMax,
}

struct GetIdentities {
    snapshot: Arc<ValidatorListSnapshot>,
    identities: ValidatorIdentities,
    query_identities: Option<BoxStream<'static, GetRecordResult>>,
    query_identities_interval: ExponentialBackoffInterval,
    pending_events: VecDeque<ToSwarm<Event, THandlerInEvent<Behaviour>>>,
}

impl GetIdentities {
    fn on_new_snapshot(&mut self, snapshot: Arc<ValidatorListSnapshot>) {
        self.snapshot = snapshot;

        // eliminate identities that are neither in the current set nor in the next set
        self.identities
            .retain(|&address, _identity| self.snapshot.contains(address));
    }

    fn identity_keys(&self) -> impl Iterator<Item = ValidatorIdentityKey> {
        self.snapshot
            .iter()
            .map(|address| ValidatorIdentityKey { validator: address })
    }

    fn verify_record(
        &self,
        record: ValidatorIdentityRecord,
    ) -> Result<Option<SignedValidatorIdentity>, VerifyRecordError> {
        let ValidatorIdentityRecord { value: identity } = record;

        if !self.snapshot.contains(identity.address()) {
            return Err(VerifyRecordError::UnknownValidatorIdentity {
                address: identity.address(),
            });
        }

        if let Some(old_identity) = self.identities.get(&identity.address())
            && old_identity.inner.creation_time >= identity.inner.creation_time
        {
            return Ok(None);
        }

        Ok(Some(identity))
    }

    fn put_identity(&mut self, record: ValidatorIdentityRecord) -> Result<bool, VerifyRecordError> {
        let Some(identity) = self.verify_record(record)? else {
            return Ok(false);
        };

        self.identities.insert(identity.address(), identity);
        Ok(true)
    }

    fn poll(
        &mut self,
        cx: &mut Context<'_>,
        kad: &kad::Handle,
    ) -> Poll<ToSwarm<Event, THandlerInEvent<Behaviour>>> {
        if let Some(event) = self.pending_events.pop_front() {
            return Poll::Ready(event);
        }

        if self.query_identities_interval.poll_tick(cx).is_ready() {
            let streams = self
                .identity_keys()
                .map(|identity| {
                    let identity = RecordKey::ValidatorIdentity(identity);
                    kad.get_record(identity)
                })
                .collect::<Vec<_>>();
            let stream = stream::iter(streams)
                .flatten_unordered(Some(MAX_IN_FLIGHT_QUERIES))
                .boxed();
            self.query_identities.replace(stream);
            return Poll::Ready(ToSwarm::GenerateEvent(Event::GetIdentitiesStarted));
        }

        if let Some(stream) = &mut self.query_identities
            && let Poll::Ready(Some(res)) = stream.poll_next_unpin(cx)
        {
            match res {
                Ok(kad::GetRecordOk { peer: _, record }) => {
                    let record = record.unwrap_validator_identity();
                    let address = record.value.address();
                    match self.put_identity(record) {
                        Ok(true) => {
                            return Poll::Ready(ToSwarm::GenerateEvent(Event::IdentityUpdated {
                                address,
                            }));
                        }
                        Ok(false) => {
                            /* we already have a newer identity or the same one for this validator */
                        }
                        Err(err) => {
                            log::trace!("failed to save identity from get record query: {err}");
                        }
                    }
                }
                Err(err) => {
                    log::trace!("failed to query identity: {err}");
                }
            }
        }

        Poll::Pending
    }
}

#[derive(Debug, derive_more::Display, Eq, PartialEq)]
pub enum VerifyRecordError {
    #[display("unknown validator identity: {address}")]
    UnknownValidatorIdentity { address: Address },
}

struct PutIdentity {
    keypair: Keypair,
    validator_key: PublicKey,
    signer: Signer,
    interval: ExponentialBackoffInterval,
    external_addresses: ExternalAddresses,
    fut: Option<PutRecordFuture>,
}

impl PutIdentity {
    fn new_identity(&self) -> Option<anyhow::Result<ValidatorIdentityRecord>> {
        let addresses = ValidatorAddresses::from_external_addresses(
            self.keypair.public().to_peer_id(),
            &self.external_addresses,
        );
        let Some(addresses) = addresses else {
            // generally, should not be the case because bootnodes will tell us
            // our external address through the `libp2p-identify` protocol
            log::warn!("No external addresses found to generate identity");
            return None;
        };
        let creation_time = SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)
            .expect("SystemTime before UNIX EPOCH")
            .as_nanos();
        let identity = ValidatorIdentity {
            addresses,
            creation_time,
        };

        let record = identity
            .sign(&self.signer, self.validator_key, &self.keypair)
            .context("failed to sign validator identity")
            .map(|value| ValidatorIdentityRecord { value });
        Some(record)
    }

    fn poll(&mut self, cx: &mut Context<'_>, kad: &kad::Handle) -> Poll<Event> {
        if let Some(fut) = self.fut.as_mut()
            && let Poll::Ready(res) = fut.poll_unpin(cx)
        {
            self.fut = None;
            match res {
                Ok(_key) => {
                    self.interval.tick_at_max();
                    return Poll::Ready(Event::PutIdentityTicksAtMax);
                }
                Err(err) => {
                    log::trace!("failed to put validator identity: {err}");
                }
            }
        }

        ready!(self.interval.poll_tick(cx));

        let Some(identity) = self.new_identity() else {
            return Poll::Pending;
        };

        match identity {
            Ok(record) => {
                let record = kad::Record::ValidatorIdentity(record);
                // best effort; ignore result
                self.fut = Some(kad.put_record(record));
                return Poll::Ready(Event::PutIdentityStarted);
            }
            Err(err) => {
                log::trace!("failed to create identity to put: {err}");
            }
        }

        Poll::Pending
    }
}

pub struct Behaviour {
    kad: kad::Handle,
    get_identities: GetIdentities,
    put_identity: Option<PutIdentity>,
}

impl Behaviour {
    pub fn new(
        kad: kad::Handle,
        keypair: Keypair,
        validator_key: Option<PublicKey>,
        signer: Signer,
        snapshot: Arc<ValidatorListSnapshot>,
    ) -> Self {
        Self {
            kad,
            get_identities: GetIdentities {
                snapshot,
                identities: HashMap::new(),
                query_identities: None,
                query_identities_interval: ExponentialBackoffInterval::new(),
                pending_events: VecDeque::new(),
            },
            put_identity: validator_key.map(|validator_key| PutIdentity {
                keypair,
                validator_key,
                signer,
                interval: ExponentialBackoffInterval::new(),
                external_addresses: ExternalAddresses::default(),
                fut: None,
            }),
        }
    }

    pub(crate) fn on_new_snapshot(&mut self, snapshot: Arc<ValidatorListSnapshot>) {
        self.get_identities.on_new_snapshot(snapshot);
    }

    pub fn identities(&self) -> &ValidatorIdentities {
        &self.get_identities.identities
    }

    #[cfg(test)]
    pub fn get_identity(&self, address: Address) -> Option<&SignedValidatorIdentity> {
        self.get_identities.identities.get(&address)
    }

    pub fn verify_record(
        &self,
        record: ValidatorIdentityRecord,
    ) -> Result<Option<SignedValidatorIdentity>, VerifyRecordError> {
        self.get_identities.verify_record(record)
    }
}

impl NetworkBehaviour for Behaviour {
    type ConnectionHandler = dummy::ConnectionHandler;
    type ToSwarm = Event;

    fn handle_established_inbound_connection(
        &mut self,
        _connection_id: ConnectionId,
        _peer: PeerId,
        _local_addr: &Multiaddr,
        _remote_addr: &Multiaddr,
    ) -> Result<THandler<Self>, ConnectionDenied> {
        Ok(dummy::ConnectionHandler)
    }

    fn handle_established_outbound_connection(
        &mut self,
        _connection_id: ConnectionId,
        _peer: PeerId,
        _addr: &Multiaddr,
        _role_override: Endpoint,
        _port_use: PortUse,
    ) -> Result<THandler<Self>, ConnectionDenied> {
        Ok(dummy::ConnectionHandler)
    }

    fn on_swarm_event(&mut self, event: FromSwarm) {
        if let Some(put_identity) = &mut self.put_identity {
            put_identity.external_addresses.on_swarm_event(&event);
        }
    }

    fn on_connection_handler_event(
        &mut self,
        _peer_id: PeerId,
        _connection_id: ConnectionId,
        _event: THandlerOutEvent<Self>,
    ) {
    }

    fn poll(
        &mut self,
        cx: &mut Context<'_>,
    ) -> Poll<ToSwarm<Self::ToSwarm, THandlerInEvent<Self>>> {
        if let Poll::Ready(event) = self.get_identities.poll(cx, &self.kad) {
            return Poll::Ready(event);
        }

        if let Some(put_identity) = &mut self.put_identity
            && let Poll::Ready(event) = put_identity.poll(cx, &self.kad)
        {
            return Poll::Ready(ToSwarm::GenerateEvent(event));
        }

        Poll::Pending
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use assert_matches::assert_matches;
    use core::convert::TryFrom;
    use ethexe_common::ValidatorsVec;
    use libp2p::{Multiaddr, Swarm, identity::Keypair};
    use libp2p_swarm_test::SwarmExt;
    use std::sync::Arc;
    use tokio::time;

    fn test_addr() -> Multiaddr {
        "/ip4/127.0.0.1/tcp/1234".parse().unwrap()
    }

    fn new_snapshot(addresses: Vec<Address>) -> Arc<ValidatorListSnapshot> {
        Arc::new(ValidatorListSnapshot {
            current_era_index: 0,
            current_validators: ValidatorsVec::try_from(addresses)
                .expect("validator set should not be empty"),
            next_validators: None,
        })
    }

    fn new_signed_identity(
        signer: &Signer,
        validator_key: PublicKey,
        creation_time: u128,
    ) -> SignedValidatorIdentity {
        let network_keypair = Keypair::generate_secp256k1();
        let identity = ValidatorIdentity {
            addresses: ValidatorAddresses::new(network_keypair.public().to_peer_id(), test_addr()),
            creation_time,
        };
        identity
            .sign(signer, validator_key, &network_keypair)
            .expect("failed to sign validator identity")
    }

    #[test]
    fn encode_decode_identity() {
        let signer = Signer::memory();
        let validator_key = signer.generate_key().unwrap();
        let keypair = Keypair::generate_secp256k1();
        let identity = ValidatorIdentity {
            addresses: ValidatorAddresses::new(keypair.public().to_peer_id(), test_addr()),
            creation_time: 999_999,
        };
        let identity = identity.sign(&signer, validator_key, &keypair).unwrap();

        let decoded_identity =
            SignedValidatorIdentity::decode(&mut &identity.encode()[..]).unwrap();
        assert_eq!(identity, decoded_identity);
    }

    #[test]
    fn different_peer_ids_in_identity() {
        let signer = Signer::memory();
        let validator_key = signer.generate_key().unwrap();
        let keypair = Keypair::generate_secp256k1();
        let identity = ValidatorIdentity {
            addresses: ValidatorAddresses::new(PeerId::random(), test_addr()),
            creation_time: 999_999,
        };
        let identity = identity.sign(&signer, validator_key, &keypair).unwrap();

        SignedValidatorIdentity::decode(&mut &identity.encode()[..]).unwrap_err();
    }

    #[test]
    fn validator_addresses_from_vec_of_vec() {
        let addr1_peer_id = PeerId::random();
        let addr1 = test_addr().with_p2p(addr1_peer_id).unwrap();
        let addr2 = test_addr().with_p2p(PeerId::random()).unwrap();

        let addresses = ValidatorAddresses::from_vec_of_vec(vec![addr1.to_vec()]).unwrap();
        assert_eq!(addresses.peer_id(), addr1_peer_id);
        assert_eq!(addresses.into_iter().next(), Some(addr1.clone()));

        assert_matches!(
            ValidatorAddresses::from_vec_of_vec(Vec::new()).unwrap_err(),
            FromVecOfVecError::NoAddresses
        );

        assert_matches!(
            ValidatorAddresses::from_vec_of_vec(vec![vec![0xfe]; MAX_IDENTITY_ADDRESSES + 1])
                .unwrap_err(),
            FromVecOfVecError::TooManyAddresses {
                expected: MAX_IDENTITY_ADDRESSES,
                actual: 11
            }
        );

        assert_matches!(
            ValidatorAddresses::from_vec_of_vec(vec![vec![1]]).unwrap_err(),
            FromVecOfVecError::ParseMultiaddr(_)
        );

        assert_matches!(
            ValidatorAddresses::from_vec_of_vec(vec![test_addr().to_vec()]).unwrap_err(),
            FromVecOfVecError::LastProtocolIsNotP2p
        );

        assert_matches!(
            ValidatorAddresses::from_vec_of_vec(vec![Multiaddr::empty().to_vec()]).unwrap_err(),
            FromVecOfVecError::MultiaddrIsEmpty
        );

        assert_matches!(
            ValidatorAddresses::from_vec_of_vec(vec![addr1.to_vec(), addr2.to_vec()]).unwrap_err(),
            FromVecOfVecError::PeerIdMismatch { .. }
        );

        assert_matches!(
            ValidatorAddresses::from_vec_of_vec(vec![addr1.to_vec(), addr1.to_vec()]).unwrap_err(),
            FromVecOfVecError::DuplicatedMultiaddr
        )
    }

    #[tokio::test(start_paused = true)]
    async fn behaviour_queries_and_puts() {
        let signer = Signer::memory();
        let validator_key = signer.generate_key().unwrap();
        let behaviour = Behaviour::new(
            kad::Handle::new_test(),
            Keypair::generate_secp256k1(),
            Some(validator_key),
            signer.clone(),
            new_snapshot(vec![validator_key.to_address()]),
        );

        let mut swarm = Swarm::new_ephemeral_tokio(move |_keypair| behaviour);
        swarm.add_external_address(test_addr());

        time::advance(ExponentialBackoffInterval::START).await;

        let event = swarm.next_behaviour_event().await;
        assert_matches!(event, Event::GetIdentitiesStarted);

        let event = swarm.next_behaviour_event().await;
        assert_matches!(event, Event::PutIdentityStarted);
    }

    #[tokio::test(start_paused = true)]
    async fn behaviour_stores_identity_for_known_validator() {
        let signer = Signer::memory();
        let validator_key = signer.generate_key().unwrap();
        let identity = new_signed_identity(&signer, validator_key, 10);

        let (kad_handle, mut kad_callback) = kad::test_utils::HandleCallback::new_pair();
        let identity_clone = identity.clone();
        kad_callback.on_get_record(move |_key| {
            Ok(kad::GetRecordOk {
                peer: None,
                record: ValidatorIdentityRecord {
                    value: identity_clone.clone(),
                }
                .into(),
            })
        });
        tokio::spawn(kad_callback.loop_on_receiver());

        let behaviour = Behaviour::new(
            kad_handle,
            Keypair::generate_secp256k1(),
            None,
            signer.clone(),
            new_snapshot(vec![validator_key.to_address()]),
        );
        let mut swarm = Swarm::new_ephemeral_tokio(|_keypair| behaviour);

        let event = swarm.next_behaviour_event().await;
        assert_eq!(event, Event::GetIdentitiesStarted);

        let event = swarm.next_behaviour_event().await;
        assert_eq!(
            event,
            Event::IdentityUpdated {
                address: validator_key.to_address()
            }
        );
        assert_eq!(
            swarm.behaviour().get_identity(validator_key.to_address()),
            Some(&identity)
        );
    }

    #[tokio::test]
    async fn verify_record_rejects_unknown_validator() {
        let signer = Signer::memory();
        let validator_key = signer.generate_key().unwrap();
        let identity = new_signed_identity(&signer, validator_key, 10);

        let behaviour = Behaviour::new(
            kad::Handle::new_test(),
            Keypair::generate_secp256k1(),
            None,
            signer.clone(),
            new_snapshot(vec![Address::from(1u64)]),
        );

        let err = behaviour
            .get_identities
            .verify_record(ValidatorIdentityRecord { value: identity })
            .unwrap_err();
        assert_eq!(
            err,
            VerifyRecordError::UnknownValidatorIdentity {
                address: validator_key.to_address()
            }
        );
        assert!(behaviour.get_identity(validator_key.to_address()).is_none());
    }

    #[tokio::test]
    async fn put_identity_prefers_newer_records() {
        let signer = Signer::memory();
        let validator_key = signer.generate_key().unwrap();
        let mut behaviour = Behaviour::new(
            kad::Handle::new_test(),
            Keypair::generate_secp256k1(),
            None,
            signer.clone(),
            new_snapshot(vec![validator_key.to_address()]),
        );

        let baseline = new_signed_identity(&signer, validator_key, 20);
        behaviour
            .get_identities
            .put_identity(ValidatorIdentityRecord {
                value: baseline.clone(),
            })
            .unwrap();

        let older = new_signed_identity(&signer, validator_key, 5);
        behaviour
            .get_identities
            .put_identity(ValidatorIdentityRecord { value: older })
            .unwrap();
        assert_eq!(
            behaviour
                .get_identity(validator_key.to_address())
                .unwrap()
                .inner
                .creation_time,
            20
        );

        let newer = new_signed_identity(&signer, validator_key, 30);
        behaviour
            .get_identities
            .put_identity(ValidatorIdentityRecord {
                value: newer.clone(),
            })
            .unwrap();
        assert_eq!(
            behaviour
                .get_identity(validator_key.to_address())
                .unwrap()
                .inner
                .creation_time,
            30
        );
    }

    #[tokio::test]
    async fn on_new_snapshot_drops_obsolete_identities() {
        let signer = Signer::memory();
        let validator_a = signer.generate_key().unwrap();
        let validator_b = signer.generate_key().unwrap();
        let mut behaviour = Behaviour::new(
            kad::Handle::new_test(),
            Keypair::generate_secp256k1(),
            None,
            signer.clone(),
            new_snapshot(vec![validator_a.to_address(), validator_b.to_address()]),
        );

        let identity_a = new_signed_identity(&signer, validator_a, 10);
        let identity_b = new_signed_identity(&signer, validator_b, 10);

        behaviour
            .get_identities
            .put_identity(ValidatorIdentityRecord { value: identity_a })
            .unwrap();
        behaviour
            .get_identities
            .put_identity(ValidatorIdentityRecord { value: identity_b })
            .unwrap();

        behaviour.on_new_snapshot(new_snapshot(vec![validator_b.to_address()]));

        assert!(
            !behaviour
                .get_identities
                .identities
                .contains_key(&validator_a.to_address())
        );
        assert!(
            behaviour
                .get_identities
                .identities
                .contains_key(&validator_b.to_address())
        );
    }

    #[tokio::test(start_paused = true)]
    async fn duplicate_and_self_identity_handling() {
        let signer = Signer::memory();
        let validator_key = signer.generate_key().unwrap();
        let network_keypair = Keypair::generate_secp256k1();

        let mut behaviour = Behaviour::new(
            kad::Handle::new_test(),
            network_keypair.clone(),
            Some(validator_key),
            signer.clone(),
            new_snapshot(vec![validator_key.to_address()]),
        );

        // Create our own identity (simulating receiving our own record)
        let our_identity = ValidatorIdentity {
            addresses: ValidatorAddresses::new(network_keypair.public().to_peer_id(), test_addr()),
            creation_time: 10,
        };
        let our_signed_identity = our_identity
            .clone()
            .sign(&signer, validator_key, &network_keypair)
            .unwrap();

        // NOTE: consider ignoring our own identity
        behaviour
            .get_identities
            .put_identity(ValidatorIdentityRecord {
                value: our_signed_identity.clone(),
            })
            .unwrap();
        assert!(behaviour.get_identity(validator_key.to_address()).is_some());

        // Test duplicate records from different network keys (same validator)
        let other_network_keypair = Keypair::generate_secp256k1();
        let duplicate_identity = our_identity
            .sign(&signer, validator_key, &other_network_keypair)
            .unwrap();

        behaviour
            .get_identities
            .put_identity(ValidatorIdentityRecord {
                value: duplicate_identity,
            })
            .unwrap();

        // Should keep the newer one (or the first one if timestamps are equal)
        // Currently implementation keeps the first one for equal timestamps
        assert!(behaviour.get_identity(validator_key.to_address()).is_some());
    }

    #[tokio::test(start_paused = true)]
    async fn put_identity_ticks_at_max() {
        let signer = Signer::memory();
        let validator_key = signer.generate_key().unwrap();
        let network_keypair = Keypair::generate_secp256k1();

        let (kad_handle, mut kad_callback) = kad::test_utils::HandleCallback::new_pair();
        kad_callback.on_put_record(|record| {
            let key = record.key();
            let _ = record.unwrap_validator_identity();
            Ok(key)
        });
        tokio::spawn(kad_callback.loop_on_receiver());

        let behaviour = Behaviour::new(
            kad_handle,
            network_keypair.clone(),
            Some(validator_key),
            signer.clone(),
            new_snapshot(vec![validator_key.to_address()]),
        );
        let mut swarm = Swarm::new_ephemeral_tokio(move |_keypair| behaviour);
        swarm.add_external_address(test_addr());

        time::advance(ExponentialBackoffInterval::START).await;

        let event = swarm.next_behaviour_event().await;
        assert_eq!(event, Event::GetIdentitiesStarted);

        let event = swarm.next_behaviour_event().await;
        assert_eq!(event, Event::PutIdentityStarted);
        let put_identity = &swarm.behaviour().put_identity.as_ref().unwrap();
        assert!(put_identity.fut.is_some());

        let event = swarm.next_behaviour_event().await;
        assert_eq!(event, Event::PutIdentityTicksAtMax);
        let put_identity = &swarm.behaviour().put_identity.as_ref().unwrap();
        assert_eq!(
            put_identity.interval.period(),
            ExponentialBackoffInterval::MAX
        );
        assert!(put_identity.fut.is_none());
    }
}
