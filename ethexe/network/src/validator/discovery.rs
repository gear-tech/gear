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
    validator::list::ValidatorList,
};
use anyhow::Context;
use ethexe_common::{
    Address, ToDigest,
    ecdsa::{PublicKey, SignedData},
    sha3::Keccak256,
};
use ethexe_signer::Signer;
use libp2p::{
    Multiaddr,
    core::{Endpoint, PeerRecord, SignedEnvelope, transport::PortUse},
    identity::Keypair,
    swarm::{
        ConnectionDenied, ConnectionId, ExternalAddresses, FromSwarm, NetworkBehaviour, THandler,
        THandlerInEvent, THandlerOutEvent, ToSwarm, dummy,
    },
};
use lru::LruCache;
use parity_scale_codec::{Decode, Encode, Input, Output};
use std::{num::NonZeroUsize, task::Poll, time::SystemTime};

const MAX_VALIDATOR_IDENTITIES: NonZeroUsize = NonZeroUsize::new(100).unwrap();

pub type SignedValidatorIdentity = SignedData<ValidatorIdentity>;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ValidatorIdentity {
    pub peer_record: PeerRecord,
    pub era_index: u64,
    pub creation_time: u128,
}

impl ToDigest for ValidatorIdentity {
    fn update_hasher(&self, hasher: &mut Keccak256) {
        let Self {
            peer_record,
            era_index,
            creation_time,
        } = self;

        peer_record
            .to_signed_envelope()
            .into_protobuf_encoding()
            .update_hasher(hasher);
        era_index.to_be_bytes().update_hasher(hasher);
        creation_time.to_be_bytes().update_hasher(hasher);
    }
}

impl Encode for ValidatorIdentity {
    fn encode_to<T: Output + ?Sized>(&self, dest: &mut T) {
        let Self {
            peer_record,
            era_index,
            creation_time,
        } = self;

        let peer_record: Vec<u8> = peer_record.to_signed_envelope().into_protobuf_encoding();

        peer_record.encode_to(dest);
        era_index.encode_to(dest);
        creation_time.encode_to(dest);
    }
}

impl Decode for ValidatorIdentity {
    fn decode<I: Input>(input: &mut I) -> Result<Self, parity_scale_codec::Error> {
        let peer_record = Vec::decode(input)?;
        let peer_record = SignedEnvelope::from_protobuf_encoding(&peer_record).map_err(|e| {
            parity_scale_codec::Error::from("failed to decode peer record signed envelope")
                .chain(e.to_string())
        })?;
        let peer_record = PeerRecord::from_signed_envelope(peer_record).map_err(|e| {
            parity_scale_codec::Error::from("failed to decode peer record").chain(e.to_string())
        })?;

        let era_index = u64::decode(input)?;
        let creation_time = u128::decode(input)?;

        Ok(Self {
            peer_record,
            era_index,
            creation_time,
        })
    }
}

#[derive(Debug)]
pub enum Event {
    QueryIdentities,
    PutIdentity,
}

#[derive(Debug)]
pub struct Behaviour {
    keypair: Keypair,
    validator_key: Option<PublicKey>,
    signer: Signer,
    identities: LruCache<Address, SignedValidatorIdentity>,
    external_addresses: ExternalAddresses,
    query_identities_interval: ExponentialBackoffInterval,
    put_identity_interval: ExponentialBackoffInterval,
}

impl Behaviour {
    pub fn new(keypair: Keypair, validator_key: Option<PublicKey>, signer: Signer) -> Self {
        Self {
            keypair,
            validator_key,
            signer,
            identities: LruCache::new(MAX_VALIDATOR_IDENTITIES),
            external_addresses: ExternalAddresses::default(),
            query_identities_interval: ExponentialBackoffInterval::new(),
            put_identity_interval: ExponentialBackoffInterval::new(),
        }
    }

    pub fn identity_keys(
        &self,
        list: &ValidatorList,
    ) -> impl Iterator<Item = ValidatorIdentityKey> {
        let current_era = list.current_era_index();
        list.current_validators()
            .map(move |address| ValidatorIdentityKey {
                current_era,
                validator: address,
            })
    }

    pub fn identity(
        &self,
        list: &ValidatorList,
    ) -> Option<anyhow::Result<ValidatorIdentityRecord>> {
        let validator_key = self.validator_key?;

        let f = || {
            let current_era = list.current_era_index();

            let peer_record = self.external_addresses.as_slice().to_vec();
            let peer_record = PeerRecord::new(&self.keypair, peer_record)
                .context("failed to sign peer record")?;

            let creation_time = SystemTime::now()
                .duration_since(SystemTime::UNIX_EPOCH)
                .expect("SystemTime before UNIX EPOCH")
                .as_nanos();

            let identity = ValidatorIdentity {
                peer_record,
                era_index: current_era,
                creation_time,
            };
            let identity = self
                .signer
                .signed_data(validator_key, identity)
                .context("failed to sign validator identity")?;

            let key = ValidatorIdentityKey {
                current_era,
                validator: validator_key.to_address(),
            };
            let record = ValidatorIdentityRecord {
                key,
                value: identity,
            };
            Ok(record)
        };

        Some(f())
    }

    pub fn get_identity(&mut self, address: Address) -> Option<&SignedValidatorIdentity> {
        self.identities.get(&address)
    }

    pub fn put_identity(
        &mut self,
        list: &ValidatorList,
        record: ValidatorIdentityRecord,
    ) -> anyhow::Result<()> {
        log::error!("validator identity record: {record:?}");

        let ValidatorIdentityRecord {
            key: _,
            value: identity,
        } = record;

        anyhow::ensure!(
            list.contains_any_validator(identity.address()),
            "received identity is not in any validator list"
        );

        anyhow::ensure!(
            identity.data().era_index == list.current_era_index()
                || identity.data().era_index == list.current_era_index() + 1,
            "received identity has invalid era index"
        );

        if let Some(old_identity) = self.identities.peek(&identity.address())
            && old_identity.data().creation_time >= identity.data().creation_time
        {
            return Ok(());
        }

        self.identities.put(identity.address(), identity);

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
            return Poll::Ready(ToSwarm::GenerateEvent(Event::QueryIdentities));
        }

        if self.put_identity_interval.poll_tick(cx).is_ready() {
            return Poll::Ready(ToSwarm::GenerateEvent(Event::PutIdentity));
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
        let keypair = Keypair::generate_secp256k1();
        let identity = ValidatorIdentity {
            peer_record: PeerRecord::new(&keypair, vec![]).unwrap(),
            era_index: 123,
            creation_time: 999_999,
        };

        let decoded_identity = ValidatorIdentity::decode(&mut &identity.encode()[..]).unwrap();
        assert_eq!(identity, decoded_identity);
    }
}
