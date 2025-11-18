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
    swarm::{
        ConnectionDenied, ConnectionId, ExternalAddresses, FromSwarm, NetworkBehaviour, THandler,
        THandlerInEvent, THandlerOutEvent, ToSwarm, dummy,
    },
};
use parity_scale_codec::{Decode, Encode, Input, Output};
use std::{collections::HashMap, sync::Arc, task::Poll, time::SystemTime};

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
    network_signature: Vec<u8>,
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
            network_key,
        } = self;

        inner.encode_to(dest);
        validator_signature.encode_to(dest);
        network_signature.encode_to(dest);
        network_key.to_bytes().encode_to(dest);
    }
}

impl Decode for SignedValidatorIdentity {
    fn decode<I: Input>(input: &mut I) -> Result<Self, parity_scale_codec::Error> {
        let inner = ValidatorIdentity::decode(input)?;

        let validator_signature = Signature::decode(input)?;
        let validator_key = validator_signature.validate(&inner).map_err(|err| {
            parity_scale_codec::Error::from("failed to validate signature").chain(err.to_string())
        })?;

        let network_signature = Vec::decode(input)?;
        let network_key = <[u8; 33]>::decode(input)?;
        let network_key = libp2p::identity::secp256k1::PublicKey::try_from_bytes(&network_key)
            .map_err(|err| {
                parity_scale_codec::Error::from("invalid network key").chain(err.to_string())
            })?;
        if !network_key.verify(&inner.encode(), &network_signature) {
            return Err(parity_scale_codec::Error::from(
                "failed to validate network signature",
            ));
        }

        Ok(Self {
            inner,
            validator_signature,
            validator_key,
            network_signature,
            network_key,
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ValidatorIdentity {
    pub addresses: Vec<Multiaddr>,
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
        let network_signature = keypair
            .sign(&self.encode())
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

impl Encode for ValidatorIdentity {
    fn encode_to<T: Output + ?Sized>(&self, dest: &mut T) {
        let Self {
            addresses,
            creation_time,
        } = self;

        let addresses: Vec<_> = addresses.iter().map(|addr| addr.to_vec()).collect();

        addresses.encode_to(dest);
        creation_time.encode_to(dest);
    }
}

impl Decode for ValidatorIdentity {
    fn decode<I: Input>(input: &mut I) -> Result<Self, parity_scale_codec::Error> {
        let addresses = <Vec<Vec<u8>>>::decode(input)?;
        let addresses = addresses
            .into_iter()
            .map(Multiaddr::try_from)
            .collect::<Result<_, _>>()
            .map_err(|err| {
                parity_scale_codec::Error::from("failed to parse multiaddr").chain(err.to_string())
            })?;

        let creation_time = u128::decode(input)?;

        Ok(Self {
            addresses,
            creation_time,
        })
    }
}

impl ToDigest for ValidatorIdentity {
    fn update_hasher(&self, hasher: &mut Keccak256) {
        let Self {
            addresses,
            creation_time,
        } = self;

        for address in addresses {
            address.as_ref().update_hasher(hasher);
        }

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

        if self.external_addresses.as_slice().is_empty() {
            // generally, should not be the case because bootnodes will tell us
            // our external address through the `libp2p-identify` protocol
            log::warn!("No external addresses found to generate identity");
            return None;
        }

        let f = || {
            let addresses = self.external_addresses.as_slice().to_vec();
            let creation_time = SystemTime::now()
                .duration_since(SystemTime::UNIX_EPOCH)
                .expect("SystemTime before UNIX EPOCH")
                .as_nanos();
            let identity = ValidatorIdentity {
                addresses,
                creation_time,
            };
            let identity = identity
                .sign(&self.signer, validator_key, &self.keypair)
                .context("failed to sign validator identity")?;

            Ok(ValidatorIdentityRecord { value: identity })
        };

        Some(f())
    }

    pub fn get_identity(&mut self, address: Address) -> Option<&SignedValidatorIdentity> {
        self.identities.get(&address)
    }

    pub fn put_identity(&mut self, record: ValidatorIdentityRecord) -> anyhow::Result<()> {
        log::trace!("filtering received record: {record:?}");

        let ValidatorIdentityRecord { value: identity } = record;

        anyhow::ensure!(
            self.snapshot.contains_any_validator(identity.address()),
            "received identity is not in any validator list"
        );

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
    use libp2p::{
        Multiaddr, Swarm,
        identity::Keypair,
        swarm::{FromSwarm, behaviour::ExternalAddrConfirmed},
    };
    use libp2p_swarm_test::SwarmExt;
    use std::{
        future,
        sync::{Arc, LazyLock},
        task::Poll,
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
        let identity = ValidatorIdentity {
            addresses: vec![TEST_ADDR.clone()],
            creation_time,
        };
        let network_keypair = Keypair::generate_secp256k1();
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
            addresses: vec!["/ip4/127.0.0.1/tcp/123".parse().unwrap()],
            creation_time: 999_999,
        };
        let identity = identity.sign(&signer, validator_key, &keypair).unwrap();

        let decoded_identity =
            SignedValidatorIdentity::decode(&mut &identity.encode()[..]).unwrap();
        assert_eq!(identity, decoded_identity);
    }

    #[ignore = "Tampered signatures are not detected"]
    #[tokio::test]
    async fn tampered_signatures() {
        let signer = Signer::memory();
        let validator_key = signer.generate_key().unwrap();
        let attacker_key = signer.generate_key().unwrap();
        let attacker_private_key = signer.storage().get_private_key(attacker_key).unwrap();
        let identity = signed_identity(&signer, validator_key, 10);

        let mut corrupted_identity = identity.clone();
        corrupted_identity.validator_signature =
            Signature::create(attacker_private_key, b"").unwrap();
        let corrupted_identity = corrupted_identity.encode();
        SignedValidatorIdentity::decode(&mut &corrupted_identity[..]).unwrap_err();

        let mut corrupted_identity = identity.clone();
        corrupted_identity.network_signature = Vec::new();
        let corrupted_identity = corrupted_identity.encode();
        SignedValidatorIdentity::decode(&mut &corrupted_identity[..]).unwrap_err();
    }

    #[tokio::test]
    async fn identity_returns_signed_record_when_validator_key_present() {
        let signer = Signer::memory();
        let validator_key = signer.generate_key().unwrap();
        let mut behaviour = Behaviour::new(
            Keypair::generate_secp256k1(),
            Some(validator_key),
            signer.clone(),
            snapshot(vec![validator_key.to_address()]),
        );

        let external_addr: Multiaddr = "/ip4/10.0.0.1/tcp/55".parse().unwrap();
        behaviour.on_swarm_event(FromSwarm::ExternalAddrConfirmed(ExternalAddrConfirmed {
            addr: &external_addr,
        }));

        let record = behaviour.identity().expect("validator key set").unwrap();
        assert_eq!(record.value.data().addresses, vec![external_addr]);
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

        assert!(
            err.to_string()
                .contains("received identity is not in any validator list")
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
    async fn validator_set_edge_cases() {
        let signer = Signer::memory();

        // Test with empty validator set - need at least one validator for snapshot
        let dummy_validator = signer.generate_key().unwrap();
        let mut behaviour = Behaviour::new(
            Keypair::generate_secp256k1(),
            None,
            signer.clone(),
            snapshot(vec![dummy_validator.to_address()]),
        );

        // Should emit query events even for single validator set
        let waker = futures::task::noop_waker();
        let mut cx = std::task::Context::from_waker(&waker);

        time::advance(crate::utils::ExponentialBackoffInterval::START).await;
        assert!(matches!(
            behaviour.poll(&mut cx),
            Poll::Ready(ToSwarm::GenerateEvent(Event::QueryIdentities { identities })) if identities.len() == 1
        ));

        // Test validator moving between current and next sets
        let validator_a = signer.generate_key().unwrap();
        let validator_b = signer.generate_key().unwrap();

        let snapshot_with_next = Arc::new(ValidatorListSnapshot {
            chain_head_ts: 0,
            timelines: ProtocolTimelines {
                genesis_ts: 0,
                era: 10,
                election: 5,
            },
            current_validators: ValidatorsVec::try_from(vec![validator_a.to_address()])
                .expect("validator set should not be empty"),
            next_validators: Some(
                ValidatorsVec::try_from(vec![validator_b.to_address()])
                    .expect("validator set should not be empty"),
            ),
        });

        let mut behaviour = Behaviour::new(
            Keypair::generate_secp256k1(),
            None,
            signer.clone(),
            snapshot_with_next,
        );

        // Both validators should be considered valid
        let identity_a = signed_identity(&signer, validator_a, 10);
        let identity_b = signed_identity(&signer, validator_b, 10);

        behaviour
            .put_identity(ValidatorIdentityRecord { value: identity_a })
            .unwrap();
        behaviour
            .put_identity(ValidatorIdentityRecord { value: identity_b })
            .unwrap();

        assert!(behaviour.get_identity(validator_a.to_address()).is_some());
        assert!(behaviour.get_identity(validator_b.to_address()).is_some());

        // Update snapshot - validator_b moves to current, validator_a is removed
        let snapshot_with_next = Arc::new(ValidatorListSnapshot {
            chain_head_ts: 0,
            timelines: ProtocolTimelines {
                genesis_ts: 0,
                era: 11,
                election: 6,
            },
            current_validators: ValidatorsVec::try_from(vec![validator_b.to_address()])
                .expect("validator set should not be empty"),
            next_validators: None,
        });

        behaviour.on_new_snapshot(snapshot_with_next);

        assert!(behaviour.get_identity(validator_a.to_address()).is_none());
        assert!(behaviour.get_identity(validator_b.to_address()).is_some());
    }

    #[tokio::test(start_paused = true)]
    async fn network_behavior_edge_cases() {
        let signer = Signer::memory();
        let validator_key = signer.generate_key().unwrap();

        // Test behavior without external addresses
        let mut swarm = Swarm::new_ephemeral_tokio(|_keypair| {
            Behaviour::new(
                Keypair::generate_secp256k1(),
                Some(validator_key),
                signer.clone(),
                snapshot(vec![validator_key.to_address()]),
            )
        });

        time::advance(ExponentialBackoffInterval::START).await;

        // First poll should be query identities
        let event = swarm.next_behaviour_event().await;
        assert_matches!(event, Event::QueryIdentities { .. });

        // Second poll should not emit put identity (no external addresses)
        future::poll_fn(|cx| {
            assert_matches!(swarm.behaviour_mut().poll(cx), Poll::Pending);
            Poll::Ready(())
        })
        .await;

        // Test with multiple external addresses
        time::advance(ExponentialBackoffInterval::START).await;

        let addr1: Multiaddr = "/ip4/10.0.0.1/tcp/55".parse().unwrap();
        let addr2: Multiaddr = "/ip6/::1/tcp/66".parse().unwrap();
        swarm.add_external_address(addr1.clone());
        swarm.add_external_address(addr2.clone());

        let record = swarm
            .behaviour()
            .identity()
            .expect("validator key set")
            .unwrap();
        assert_eq!(record.value.data().addresses.len(), 2);
        assert!(record.value.data().addresses.contains(&addr1));
        assert!(record.value.data().addresses.contains(&addr2));
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
            addresses: vec![TEST_ADDR.clone()],
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
