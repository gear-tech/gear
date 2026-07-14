// Copyright (C) Gear Technologies Inc.
// SPDX-License-Identifier: GPL-3.0-or-later WITH Classpath-exception-2.0

use crate::malachite::Config;
use anyhow::anyhow;
use bytes::Bytes;
use either::Either;
use libp2p::{
    Multiaddr, PeerId,
    core::{Endpoint, transport::PortUse},
    request_response,
    swarm::{
        ConnectionDenied, ConnectionHandler, ConnectionHandlerSelect, ConnectionId, FromSwarm,
        NetworkBehaviour, THandler, THandlerInEvent, THandlerOutEvent, ToSwarm,
    },
};
use libp2p_broadcast as broadcast;
use malachitebft_network::{self as network, validator_proof};
use malachitebft_sync::{self as sync, ResponseChannel};
use std::task::{Context, Poll};

#[derive(Debug, derive_more::From)]
pub(crate) enum Event {
    Broadcast(broadcast::Event),
    Sync(sync::Event),
    ValidatorProof(validator_proof::Event),
}

pub(crate) struct Behaviour {
    pub broadcast: broadcast::Behaviour,
    pub sync: sync::Behaviour,
    pub validator_proof: validator_proof::Behaviour,
    sync_broadcast_topic: broadcast::Topic,
}

impl Behaviour {
    pub(crate) fn new(
        config: &Config,
        registry: &mut libp2p::metrics::Registry,
    ) -> anyhow::Result<Self> {
        let mut broadcast = broadcast::Behaviour::new_with_metrics(
            broadcast::Config {
                max_buf_size: config.pubsub_max_size as usize,
            },
            registry.sub_registry_with_prefix("malachite_broadcast"),
        );
        let sync_broadcast_topic = network::Channel::Sync.to_broadcast_topic(config.channel_names);
        broadcast.subscribe(sync_broadcast_topic);

        let sync = sync::Behaviour::new(
            sync::Config::default().with_max_response_size(config.rpc_max_size as usize),
            config.protocol_names.sync.clone(),
        )
        .map_err(|e| anyhow!("`malachite sync behaviour` error: {e:?}"))?;

        let validator_proof_protocol =
            libp2p::StreamProtocol::try_from_owned(config.protocol_names.validator_proof.clone())?;
        let validator_proof = validator_proof::Behaviour::new(validator_proof_protocol);

        Ok(Self {
            broadcast,
            sync,
            validator_proof,
            sync_broadcast_topic,
        })
    }

    pub(crate) fn broadcast_status(&mut self, data: Bytes) {
        self.broadcast.broadcast(&self.sync_broadcast_topic, data);
    }

    pub(crate) fn send_sync_request(
        &mut self,
        peer: PeerId,
        body: Bytes,
    ) -> request_response::OutboundRequestId {
        self.sync.send_request(peer, body)
    }

    pub(crate) fn send_sync_response(&mut self, channel: ResponseChannel, body: Bytes) {
        if let Err(error) = self.sync.send_response(channel, body) {
            log::warn!("failed to send Malachite sync response: {error}");
        }
    }
}

impl NetworkBehaviour for Behaviour {
    type ConnectionHandler = ConnectionHandlerSelect<
        ConnectionHandlerSelect<THandler<broadcast::Behaviour>, THandler<sync::Behaviour>>,
        THandler<validator_proof::Behaviour>,
    >;
    type ToSwarm = Event;

    fn handle_pending_inbound_connection(
        &mut self,
        connection_id: ConnectionId,
        local_addr: &Multiaddr,
        remote_addr: &Multiaddr,
    ) -> Result<(), ConnectionDenied> {
        let Self {
            broadcast,
            sync,
            validator_proof,
            sync_broadcast_topic: _,
        } = self;

        broadcast.handle_pending_inbound_connection(connection_id, local_addr, remote_addr)?;
        sync.handle_pending_inbound_connection(connection_id, local_addr, remote_addr)?;
        validator_proof.handle_pending_inbound_connection(
            connection_id,
            local_addr,
            remote_addr,
        )?;
        Ok(())
    }

    fn handle_established_inbound_connection(
        &mut self,
        connection_id: ConnectionId,
        peer: PeerId,
        local_addr: &Multiaddr,
        remote_addr: &Multiaddr,
    ) -> Result<THandler<Self>, ConnectionDenied> {
        let Self {
            broadcast,
            sync,
            validator_proof,
            sync_broadcast_topic: _,
        } = self;

        let broadcast_handler = broadcast.handle_established_inbound_connection(
            connection_id,
            peer,
            local_addr,
            remote_addr,
        )?;
        let sync_handler = sync.handle_established_inbound_connection(
            connection_id,
            peer,
            local_addr,
            remote_addr,
        )?;
        let validator_proof_handler = validator_proof.handle_established_inbound_connection(
            connection_id,
            peer,
            local_addr,
            remote_addr,
        )?;

        Ok(broadcast_handler
            .select(sync_handler)
            .select(validator_proof_handler))
    }

    fn handle_pending_outbound_connection(
        &mut self,
        connection_id: ConnectionId,
        maybe_peer: Option<PeerId>,
        addresses: &[Multiaddr],
        effective_role: Endpoint,
    ) -> Result<Vec<Multiaddr>, ConnectionDenied> {
        let Self {
            broadcast,
            sync,
            validator_proof,
            sync_broadcast_topic: _,
        } = self;

        let broadcast_addresses = broadcast.handle_pending_outbound_connection(
            connection_id,
            maybe_peer,
            addresses,
            effective_role,
        )?;
        let sync_addresses = sync.handle_pending_outbound_connection(
            connection_id,
            maybe_peer,
            addresses,
            effective_role,
        )?;
        let validator_proof_addresses = validator_proof.handle_pending_outbound_connection(
            connection_id,
            maybe_peer,
            addresses,
            effective_role,
        )?;

        Ok([
            broadcast_addresses,
            sync_addresses,
            validator_proof_addresses,
        ]
        .concat())
    }

    fn handle_established_outbound_connection(
        &mut self,
        connection_id: ConnectionId,
        peer: PeerId,
        addr: &Multiaddr,
        role_override: Endpoint,
        port_use: PortUse,
    ) -> Result<THandler<Self>, ConnectionDenied> {
        let Self {
            broadcast,
            sync,
            validator_proof,
            sync_broadcast_topic: _,
        } = self;

        let broadcast_handler = broadcast.handle_established_outbound_connection(
            connection_id,
            peer,
            addr,
            role_override,
            port_use,
        )?;
        let sync_handler = sync.handle_established_outbound_connection(
            connection_id,
            peer,
            addr,
            role_override,
            port_use,
        )?;
        let validator_proof_handler = validator_proof.handle_established_outbound_connection(
            connection_id,
            peer,
            addr,
            role_override,
            port_use,
        )?;

        Ok(broadcast_handler
            .select(sync_handler)
            .select(validator_proof_handler))
    }

    fn on_swarm_event(&mut self, event: FromSwarm) {
        let Self {
            broadcast,
            sync,
            validator_proof,
            sync_broadcast_topic: _,
        } = self;

        broadcast.on_swarm_event(event);
        sync.on_swarm_event(event);
        validator_proof.on_swarm_event(event);
    }

    fn on_connection_handler_event(
        &mut self,
        peer_id: PeerId,
        connection_id: ConnectionId,
        event: THandlerOutEvent<Self>,
    ) {
        let Self {
            broadcast,
            sync,
            validator_proof,
            sync_broadcast_topic: _,
        } = self;

        match event {
            Either::Left(Either::Left(event)) => {
                broadcast.on_connection_handler_event(peer_id, connection_id, event)
            }
            Either::Left(Either::Right(event)) => {
                sync.on_connection_handler_event(peer_id, connection_id, event)
            }
            Either::Right(event) => {
                validator_proof.on_connection_handler_event(peer_id, connection_id, event)
            }
        }
    }

    fn poll(
        &mut self,
        cx: &mut Context<'_>,
    ) -> Poll<ToSwarm<Self::ToSwarm, THandlerInEvent<Self>>> {
        let Self {
            broadcast,
            sync,
            validator_proof,
            sync_broadcast_topic: _,
        } = self;

        if let Poll::Ready(event) = broadcast.poll(cx) {
            return Poll::Ready(
                event
                    .map_in(|message| Either::Left(Either::Left(message)))
                    .map_out(Event::Broadcast),
            );
        }

        if let Poll::Ready(event) = sync.poll(cx) {
            return Poll::Ready(
                event
                    .map_in(|message| Either::Left(Either::Right(message)))
                    .map_out(Event::Sync),
            );
        }

        if let Poll::Ready(event) = validator_proof.poll(cx) {
            return Poll::Ready(event.map_in(|x| match x {}).map_out(Event::ValidatorProof));
        }

        Poll::Pending
    }
}
