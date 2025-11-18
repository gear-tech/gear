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
    kad::{ValidatorIdentityKey, ValidatorIdentityRecord},
    utils::ExponentialBackoffInterval,
    validator::list::ValidatorListSnapshot,
};
use anyhow::Context;
use ethexe_common::{
    Address, ToDigest,
    ecdsa::{PublicKey, Signature},
    sha3::Keccak256,
};
use ethexe_signer::Signer;
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
use parity_scale_codec::{Decode, Encode, Error, Input, Output};
use std::{
    collections::{HashMap, HashSet},
    sync::Arc,
    task::Poll,
    time::SystemTime,
};

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
    pub(crate) fn data(&self) -> &ValidatorIdentity {
        &self.inner
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
        let network_key = libp2p::identity::secp256k1::PublicKey::try_from_bytes(&network_key.0)
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

/// Validator addresses
///
/// Contains at least 1 address.
/// Every address ends with P2P protocol containing the same peer ID.
///
/// Duplicated addresses are denied during decoding.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ValidatorAddresses {
    addresses: HashSet<Multiaddr>,
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

        let addresses: HashSet<Multiaddr> = addresses
            .iter()
            .cloned()
            .map(|address| {
                address
                    .with_p2p(peer_id)
                    .expect("peer ID should be the same")
            })
            .collect();

        Some(Self { addresses })
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
}

impl<const N: usize> PartialEq<[Multiaddr; N]> for ValidatorAddresses {
    fn eq(&self, other: &[Multiaddr; N]) -> bool {
        self.addresses.iter().eq(other)
    }
}

impl Encode for ValidatorAddresses {
    fn encode_to<T: Output + ?Sized>(&self, dest: &mut T) {
        let addresses: Vec<_> = self.addresses.iter().map(|addr| addr.as_ref()).collect();
        addresses.encode_to(dest);
    }
}

impl Decode for ValidatorAddresses {
    fn decode<I: Input>(input: &mut I) -> Result<Self, Error> {
        let addresses = <Vec<Vec<u8>>>::decode(input)?;

        let (addresses, _peer_id) = addresses.into_iter().try_fold(
            (HashSet::new(), None),
            |(mut set, mut peer_id), addr| {
                let addr = Multiaddr::try_from(addr).map_err(|err| {
                    parity_scale_codec::Error::from("failed to parse multiaddr")
                        .chain(err.to_string())
                })?;

                let protocol = addr
                    .iter()
                    .last()
                    .ok_or_else(|| parity_scale_codec::Error::from("address is empty"))?;
                if let multiaddr::Protocol::P2p(address_peer_id) = protocol {
                    let peer_id = *peer_id.get_or_insert(address_peer_id);
                    if peer_id != address_peer_id {
                        return Err(parity_scale_codec::Error::from("peer ID mismatch"));
                    }
                }

                if !set.insert(addr) {
                    return Err(parity_scale_codec::Error::from("duplicated address"));
                }

                Ok((set, peer_id))
            },
        )?;

        if addresses.is_empty() {
            return Err(parity_scale_codec::Error::from("empty addresses"));
        }

        Ok(Self { addresses })
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
        let validator_signature = signer
            .sign(validator_key, &self)
            .context("failed to sign validator identity with validator key")?;

        let network_private_key = keypair
            .clone()
            .try_into_secp256k1()
            .expect("we use secp256k1 for networking key")
            .secret()
            .to_bytes();
        let network_signature = Signature::create(network_private_key.into(), &self)
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

#[derive(Debug)]
pub enum Event {
    QueryIdentities {
        identities: Vec<ValidatorIdentityKey>,
    },
    PutIdentity {
        identity: anyhow::Result<Box<ValidatorIdentityRecord>>,
    },
}

#[derive(Debug, derive_more::Display, Eq, PartialEq)]
pub enum PutIdentityError {
    #[display("unknown validator identity: {address}")]
    UnknownValidatorIdentity { address: Address },
}

#[derive(Debug)]
pub struct Behaviour {
    keypair: Keypair,
    validator_key: Option<PublicKey>,
    signer: Signer,
    snapshot: Arc<ValidatorListSnapshot>,
    identities: HashMap<Address, SignedValidatorIdentity>,
    external_addresses: ExternalAddresses,
    query_identities_interval: ExponentialBackoffInterval,
    put_identity_interval: ExponentialBackoffInterval,
}

impl Behaviour {
    pub fn new(
        keypair: Keypair,
        validator_key: Option<PublicKey>,
        signer: Signer,
        snapshot: Arc<ValidatorListSnapshot>,
    ) -> Self {
        Self {
            keypair,
            validator_key,
            signer,
            snapshot,
            identities: HashMap::new(),
            external_addresses: ExternalAddresses::default(),
            query_identities_interval: ExponentialBackoffInterval::new(),
            put_identity_interval: ExponentialBackoffInterval::new(),
        }
    }

    pub(crate) fn on_new_snapshot(&mut self, snapshot: Arc<ValidatorListSnapshot>) {
        self.snapshot = snapshot;

        // eliminate identities that are neither in the current set nor in the next set
        self.identities
            .retain(|&address, _identity| self.snapshot.contains_any_validator(address));
    }

    fn identity_keys(&self) -> impl Iterator<Item = ValidatorIdentityKey> {
        self.snapshot
            .all_validators()
            .map(|address| ValidatorIdentityKey { validator: address })
    }

    fn identity(&self) -> Option<anyhow::Result<ValidatorIdentityRecord>> {
        let validator_key = self.validator_key?;

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
            .sign(&self.signer, validator_key, &self.keypair)
            .context("failed to sign validator identity")
            .map(|value| ValidatorIdentityRecord { value });
        Some(record)
    }

    pub fn get_identity(&mut self, address: Address) -> Option<&SignedValidatorIdentity> {
        self.identities.get(&address)
    }

    pub fn put_identity(
        &mut self,
        record: ValidatorIdentityRecord,
    ) -> Result<(), PutIdentityError> {
        log::trace!("filtering received record: {record:?}");

        let ValidatorIdentityRecord { value: identity } = record;

        if !self.snapshot.contains_any_validator(identity.address()) {
            return Err(PutIdentityError::UnknownValidatorIdentity {
                address: identity.address(),
            });
        }

        if let Some(old_identity) = self.identities.get(&identity.address())
            && old_identity.data().creation_time >= identity.inner.creation_time
        {
            return Ok(());
        }

        self.identities.insert(identity.address(), identity);

        Ok(())
    }

    pub fn max_put_identity_interval(&mut self) {
        self.put_identity_interval.tick_at_max();
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
        self.external_addresses.on_swarm_event(&event);
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
        cx: &mut std::task::Context<'_>,
    ) -> Poll<ToSwarm<Self::ToSwarm, THandlerInEvent<Self>>> {
        if self.query_identities_interval.poll_tick(cx).is_ready() {
            let identities = self.identity_keys().collect();
            return Poll::Ready(ToSwarm::GenerateEvent(Event::QueryIdentities {
                identities,
            }));
        }

        if self.put_identity_interval.poll_tick(cx).is_ready() {
            if let Some(identity) = self.identity() {
                let identity = identity.map(Box::new);
                return Poll::Ready(ToSwarm::GenerateEvent(Event::PutIdentity { identity }));
            } else {
                // no validator key
                self.put_identity_interval.tick_at_max();
            }
        }

        Poll::Pending
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use assert_matches::assert_matches;
    use core::convert::TryFrom;
    use ethexe_common::{ProtocolTimelines, ValidatorsVec};
    use libp2p::{Multiaddr, Swarm, identity::Keypair};
    use libp2p_swarm_test::SwarmExt;
    use std::{
        str::FromStr,
        sync::{Arc, LazyLock},
    };
    use tokio::time;

    static TEST_ADDR: LazyLock<Multiaddr> =
        LazyLock::new(|| "/ip4/127.0.0.1/tcp/1234".parse().unwrap());

    fn snapshot(addresses: Vec<Address>) -> Arc<ValidatorListSnapshot> {
        Arc::new(ValidatorListSnapshot {
            chain_head_ts: 0,
            timelines: ProtocolTimelines {
                genesis_ts: 0,
                era: 10,
                election: 5,
            },
            current_validators: ValidatorsVec::try_from(addresses)
                .expect("validator set should not be empty"),
            next_validators: None,
        })
    }

    fn signed_identity(
        signer: &Signer,
        validator_key: PublicKey,
        creation_time: u128,
    ) -> SignedValidatorIdentity {
        let network_keypair = Keypair::generate_secp256k1();
        let identity = ValidatorIdentity {
            addresses: ValidatorAddresses::new(
                network_keypair.public().to_peer_id(),
                TEST_ADDR.clone(),
            ),
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
            addresses: ValidatorAddresses::new(
                keypair.public().to_peer_id(),
                "/ip4/127.0.0.1/tcp/123".parse().unwrap(),
            ),
            creation_time: 999_999,
        };
        let identity = identity.sign(&signer, validator_key, &keypair).unwrap();

        let decoded_identity =
            SignedValidatorIdentity::decode(&mut &identity.encode()[..]).unwrap();
        assert_eq!(identity, decoded_identity);
    }

    #[tokio::test]
    async fn identity_returns_signed_record_when_validator_key_present() {
        let keypair = Keypair::generate_secp256k1();
        let local_peer_id = keypair.public().to_peer_id();
        let signer = Signer::memory();
        let validator_key = signer.generate_key().unwrap();
        let behaviour = Behaviour::new(
            keypair,
            Some(validator_key),
            signer.clone(),
            snapshot(vec![validator_key.to_address()]),
        );
        let mut swarm = Swarm::new_ephemeral_tokio(|_keypair| behaviour);

        let external_addr = Multiaddr::from_str("/ip4/10.0.0.1/tcp/55")
            .unwrap()
            .with_p2p(local_peer_id)
            .unwrap();
        swarm.add_external_address(external_addr.clone());

        let record = swarm
            .behaviour()
            .identity()
            .expect("validator key set")
            .unwrap();
        assert_eq!(record.value.data().addresses, [external_addr]);
        assert_eq!(record.value.address(), validator_key.to_address());
    }

    #[tokio::test]
    async fn identity_returns_none_without_validator_key() {
        let signer = Signer::memory();
        let validator_key = signer.generate_key().unwrap();
        let behaviour = Behaviour::new(
            Keypair::generate_secp256k1(),
            None,
            signer.clone(),
            snapshot(vec![validator_key.to_address()]),
        );

        assert!(behaviour.identity().is_none());
    }

    #[tokio::test(start_paused = true)]
    async fn poll_emits_query_and_put_events() {
        let signer = Signer::memory();
        let validator_key = signer.generate_key().unwrap();
        let behaviour = Behaviour::new(
            Keypair::generate_secp256k1(),
            Some(validator_key),
            signer.clone(),
            snapshot(vec![validator_key.to_address()]),
        );

        let mut swarm = Swarm::new_ephemeral_tokio(move |_keypair| behaviour);
        swarm.add_external_address(TEST_ADDR.clone());

        time::advance(ExponentialBackoffInterval::START).await;

        let event = swarm.next_behaviour_event().await;
        assert_matches!(event, Event::QueryIdentities { .. });

        let event = swarm.next_behaviour_event().await;
        assert_matches!(event, Event::PutIdentity { .. });
    }

    #[tokio::test]
    async fn put_identity_stores_record_for_known_validator() {
        let signer = Signer::memory();
        let validator_key = signer.generate_key().unwrap();
        let mut behaviour = Behaviour::new(
            Keypair::generate_secp256k1(),
            None,
            signer.clone(),
            snapshot(vec![validator_key.to_address()]),
        );

        let identity = signed_identity(&signer, validator_key, 10);
        behaviour
            .put_identity(ValidatorIdentityRecord {
                value: identity.clone(),
            })
            .unwrap();

        assert_eq!(
            behaviour.get_identity(validator_key.to_address()),
            Some(&identity)
        );
    }

    #[tokio::test(start_paused = true)]
    async fn put_identity_rejects_unknown_validator() {
        let signer = Signer::memory();
        let validator_key = signer.generate_key().unwrap();
        let mut behaviour = Behaviour::new(
            Keypair::generate_secp256k1(),
            None,
            signer.clone(),
            snapshot(vec![Address::from(1u64)]),
        );

        let identity = signed_identity(&signer, validator_key, 10);
        let err = behaviour
            .put_identity(ValidatorIdentityRecord { value: identity })
            .unwrap_err();

        assert_eq!(
            err,
            PutIdentityError::UnknownValidatorIdentity {
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
            Keypair::generate_secp256k1(),
            None,
            signer.clone(),
            snapshot(vec![validator_key.to_address()]),
        );

        let baseline = signed_identity(&signer, validator_key, 20);
        behaviour
            .put_identity(ValidatorIdentityRecord {
                value: baseline.clone(),
            })
            .unwrap();

        let older = signed_identity(&signer, validator_key, 5);
        behaviour
            .put_identity(ValidatorIdentityRecord { value: older })
            .unwrap();
        assert_eq!(
            behaviour
                .get_identity(validator_key.to_address())
                .unwrap()
                .data()
                .creation_time,
            20
        );

        let newer = signed_identity(&signer, validator_key, 30);
        behaviour
            .put_identity(ValidatorIdentityRecord {
                value: newer.clone(),
            })
            .unwrap();
        assert_eq!(
            behaviour
                .get_identity(validator_key.to_address())
                .unwrap()
                .data()
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
            Keypair::generate_secp256k1(),
            None,
            signer.clone(),
            snapshot(vec![validator_a.to_address(), validator_b.to_address()]),
        );

        let identity_a = signed_identity(&signer, validator_a, 10);
        let identity_b = signed_identity(&signer, validator_b, 10);

        behaviour
            .put_identity(ValidatorIdentityRecord { value: identity_a })
            .unwrap();
        behaviour
            .put_identity(ValidatorIdentityRecord { value: identity_b })
            .unwrap();

        behaviour.on_new_snapshot(snapshot(vec![validator_b.to_address()]));

        assert!(!behaviour.identities.contains_key(&validator_a.to_address()));
        assert!(behaviour.identities.contains_key(&validator_b.to_address()));
    }

    #[tokio::test(start_paused = true)]
    async fn duplicate_and_self_identity_handling() {
        let signer = Signer::memory();
        let validator_key = signer.generate_key().unwrap();
        let network_keypair = Keypair::generate_secp256k1();

        let mut behaviour = Behaviour::new(
            network_keypair.clone(),
            Some(validator_key),
            signer.clone(),
            snapshot(vec![validator_key.to_address()]),
        );

        // Create our own identity (simulating receiving our own record)
        let our_identity = ValidatorIdentity {
            addresses: ValidatorAddresses::new(
                network_keypair.public().to_peer_id(),
                TEST_ADDR.clone(),
            ),
            creation_time: 10,
        };
        let our_signed_identity = our_identity
            .clone()
            .sign(&signer, validator_key, &network_keypair)
            .unwrap();

        // TODO: Currently accepts own records (line 293 comment)
        // This should be filtered in the future
        behaviour
            .put_identity(ValidatorIdentityRecord {
                value: our_signed_identity.clone(),
            })
            .unwrap();

        // Currently accepts it, but this might change when TODO is implemented
        assert!(behaviour.get_identity(validator_key.to_address()).is_some());

        // Test duplicate records from different network keys (same validator)
        let other_network_keypair = Keypair::generate_secp256k1();
        let duplicate_identity = our_identity
            .sign(&signer, validator_key, &other_network_keypair)
            .unwrap();

        behaviour
            .put_identity(ValidatorIdentityRecord {
                value: duplicate_identity,
            })
            .unwrap();

        // Should keep the newer one (or the first one if timestamps are equal)
        // Currently implementation keeps the first one for equal timestamps
        assert!(behaviour.get_identity(validator_key.to_address()).is_some());
    }
}
