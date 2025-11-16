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
    fn sign(
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
        // TODO: filter our own record
        log::error!("validator identity record: {record:?}");

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

        if self.put_identity_interval.poll_tick(cx).is_ready()
            && let Some(identity) = self.identity()
        {
            let identity = identity.map(Box::new);
            return Poll::Ready(ToSwarm::GenerateEvent(Event::PutIdentity { identity }));
        }

        Poll::Pending
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use libp2p::identity::Keypair;

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
}
