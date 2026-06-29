// Copyright (C) Gear Technologies Inc.
// SPDX-License-Identifier: GPL-3.0-or-later WITH Classpath-exception-2.0

use crate::{
    Behaviour, malachite,
    malachite::{CoreNetworkEvent, EngineNetworkMsg, adapter::MalachiteNetworkParts},
};
use bytes::Bytes;
use ethexe_malachite_core::{MalachiteCtx, ScaleCodec};
use libp2p::{PeerId, Swarm, request_response, swarm::SwarmEvent};
use libp2p_gossipsub::MessageAcceptance;
use malachitebft_app_channel::app::types::sync;
use malachitebft_codec::Codec;
use malachitebft_core_consensus::{LivenessMsg, SignedConsensusMsg};
use malachitebft_core_types::ValidatorProof;
use malachitebft_engine::util::{output_port::OutputPort, streaming::StreamMessage};
use malachitebft_network::{PeerIdExt, ValidatorInfo};
use malachitebft_sync::Status;
use std::{
    collections::HashMap,
    task::{Context, Poll},
};
use tokio::sync::mpsc;

pub(crate) struct State {
    local_peer_id: malachitebft_app_channel::app::types::PeerId,
    rx: mpsc::Receiver<EngineNetworkMsg>,
    parts: MalachiteNetworkParts,
    validators: Vec<ValidatorInfo>,
    inbound_requests:
        HashMap<malachitebft_sync::InboundRequestId, malachitebft_sync::ResponseChannel>,
    output_port: OutputPort<CoreNetworkEvent>,
}

impl State {
    pub(crate) async fn spawn(peer_id: PeerId) -> anyhow::Result<Self> {
        let (tx, rx) = mpsc::channel(128);
        let parts = malachite::adapter::Adapter::spawn(tx).await?;

        Ok(Self {
            local_peer_id: PeerIdExt::from_libp2p(&peer_id),
            rx,
            parts,
            validators: Default::default(),
            inbound_requests: Default::default(),
            output_port: Default::default(),
        })
    }

    pub fn parts(&self) -> MalachiteNetworkParts {
        self.parts.clone()
    }

    pub fn poll(&mut self, swarm: &mut Swarm<Behaviour>, cx: &mut Context<'_>) {
        while let Poll::Ready(Some(msg)) = self.rx.poll_recv(cx) {
            self.handle_malachite_command(swarm, msg)
        }
    }

    pub fn handle_swarm_event(&mut self, event: &SwarmEvent<crate::BehaviourEvent>) {
        match event {
            SwarmEvent::ConnectionEstablished { peer_id, .. } => {
                self.output_port
                    .send(CoreNetworkEvent::PeerConnected(PeerIdExt::from_libp2p(
                        peer_id,
                    )))
            }
            SwarmEvent::ConnectionClosed {
                peer_id,
                num_established,
                ..
            } if *num_established == 0 => {
                self.output_port
                    .send(CoreNetworkEvent::PeerDisconnected(PeerIdExt::from_libp2p(
                        peer_id,
                    )))
            }
            SwarmEvent::NewListenAddr { address, .. } => self
                .output_port
                .send(CoreNetworkEvent::Listening(address.clone())),
            _ => {}
        }
    }

    pub fn handle_malachite_event(
        &mut self,
        swarm: &mut Swarm<Behaviour>,
        event: malachite::behaviour::Event,
    ) {
        log::trace!("new Malachite lane event: {event:?}");

        match event {
            malachite::behaviour::Event::Broadcast(libp2p_broadcast::Event::Received(
                peer,
                topic,
                body,
            )) => {
                let Some(malachitebft_network::Channel::Sync) =
                    malachitebft_network::Channel::from_broadcast_topic(
                        &topic,
                        malachitebft_network::ChannelNames::default(),
                    )
                else {
                    return;
                };

                let peer = malachitebft_network::PeerId::from_libp2p(&peer);

                let status: Status<MalachiteCtx> = match ScaleCodec.decode(body) {
                    Ok(request) => request,
                    Err(e) => {
                        log::error!("failed to decode sync request from {peer}: {e:?}");
                        return;
                    }
                };

                if peer != status.peer_id {
                    log::error!(
                        "Mismatched peer ID in status message: {peer} != {status_peer_id}",
                        status_peer_id = status.peer_id
                    );
                    return;
                }

                self.output_port.send(CoreNetworkEvent::Status(
                    peer,
                    malachitebft_engine::network::Status::new(
                        status.tip_height,
                        status.history_min_height,
                    ),
                ));
            }
            malachite::behaviour::Event::Sync(malachitebft_sync::Event::Message {
                peer,
                message,
                ..
            }) => match message {
                request_response::Message::Request {
                    request_id,
                    request,
                    channel,
                } => {
                    let request_id = malachitebft_sync::InboundRequestId::new(request_id);
                    self.inbound_requests.insert(request_id.clone(), channel);

                    let request = match ScaleCodec.decode(request.0) {
                        Ok(request) => request,
                        Err(e) => {
                            log::error!("failed to decode sync request from {peer}: {e:?}");
                            return;
                        }
                    };

                    self.output_port.send(CoreNetworkEvent::SyncRequest(
                        request_id,
                        PeerIdExt::from_libp2p(&peer),
                        request,
                    ));
                }
                request_response::Message::Response {
                    request_id,
                    response,
                } => {
                    let response = match ScaleCodec.decode(response.0) {
                        Ok(response) => Some(response),
                        Err(e) => {
                            log::error!("failed to decode sync response from {peer}: {e:?}");
                            None
                        }
                    };

                    self.output_port.send(CoreNetworkEvent::SyncResponse(
                        malachitebft_sync::OutboundRequestId::new(request_id),
                        PeerIdExt::from_libp2p(&peer),
                        response,
                    ));
                }
            },
            malachite::behaviour::Event::ValidatorProof(
                malachitebft_network::validator_proof::Event::ProofReceived { peer, proof_bytes },
            ) => {
                let proof: ValidatorProof<_> = match ScaleCodec.decode(proof_bytes) {
                    Ok(p) => p,
                    Err(e) => {
                        log::warn!("Failed to decode validator proof from {peer}: {e:?}, ignoring");
                        return;
                    }
                };

                // Verify peer_id in proof matches sender
                let sender_peer_id_bytes = peer.to_bytes();
                if proof.peer_id != sender_peer_id_bytes {
                    log::warn!("Invalid validator proof, disconnecting peer {peer}");
                    let _ = swarm.disconnect_peer_id(peer);
                    return;
                };

                self.output_port
                    .send(CoreNetworkEvent::ValidatorProofReceived {
                        peer_id: PeerIdExt::from_libp2p(&peer),
                        proof,
                    });
            }
            malachite::behaviour::Event::Broadcast(_)
            | malachite::behaviour::Event::Sync(_)
            | malachite::behaviour::Event::ValidatorProof(_) => {}
        }
    }

    fn handle_malachite_command(
        &mut self,
        swarm: &mut Swarm<Behaviour>,
        message: EngineNetworkMsg,
    ) {
        match message {
            EngineNetworkMsg::Subscribe(subscriber) => {
                for addr in swarm.listeners() {
                    subscriber.send(CoreNetworkEvent::Listening(addr.clone()));
                }

                for peer in swarm.connected_peers() {
                    subscriber.send(CoreNetworkEvent::PeerConnected(PeerIdExt::from_libp2p(
                        peer,
                    )));
                }

                subscriber.subscribe_to_port(&self.output_port);
            }
            EngineNetworkMsg::PublishConsensusMsg(message) => {
                let message = ScaleCodec.encode(&message).expect("encode is infallible");
                swarm
                    .behaviour_mut()
                    .gossipsub
                    .publish_malachite_consensus(message);
            }
            EngineNetworkMsg::PublishLivenessMsg(message) => {
                let message = ScaleCodec.encode(&message).expect("encode is infallible");
                swarm
                    .behaviour_mut()
                    .gossipsub
                    .publish_malachite_liveness(message);
            }
            EngineNetworkMsg::PublishProposalPart(message) => {
                let message = ScaleCodec.encode(&message).expect("encode is infallible");
                swarm
                    .behaviour_mut()
                    .gossipsub
                    .publish_malachite_proposal_part(message);
            }
            EngineNetworkMsg::BroadcastStatus(status) => {
                let status = sync::Status {
                    peer_id: self.local_peer_id,
                    tip_height: status.tip_height,
                    history_min_height: status.history_min_height,
                };
                let status = ScaleCodec.encode(&status).expect("encode is infallible");

                swarm.behaviour_mut().malachite.broadcast_status(status);
            }
            EngineNetworkMsg::OutgoingRequest(peer, request, reply) => {
                let request = ScaleCodec.encode(&request).expect("encode is infallible");
                let request_id = swarm
                    .behaviour_mut()
                    .malachite
                    .send_sync_request(peer.to_libp2p(), request);
                let malachite_request_id = malachitebft_sync::OutboundRequestId::new(request_id);
                let _ = reply.send(malachite_request_id);
            }
            EngineNetworkMsg::OutgoingResponse(request_id, response) => {
                let response = ScaleCodec.encode(&response).expect("encode is infallible");

                let channel = self
                    .inbound_requests
                    .remove(&request_id)
                    .expect("sync response has tracked inbound request id");
                swarm
                    .behaviour_mut()
                    .malachite
                    .send_sync_response(channel, response);
            }
            EngineNetworkMsg::DumpState(_reply) => {
                unreachable!("state dump is never requested in ethexe")
            }
            EngineNetworkMsg::UpdatePersistentPeers(_op, _reply) => {
                unreachable!("persistent peers update is never requested in ethexe")
            }
            EngineNetworkMsg::UpdateValidatorSet(validators) => {
                let validators: Vec<_> = validators
                    .iter()
                    .map(|v| ValidatorInfo {
                        address: v.address.to_string(),
                        public_key: v.public_key.to_vec(),
                        voting_power: v.voting_power,
                    })
                    .collect();
                self.validators = validators;
            }
            EngineNetworkMsg::ValidatorProofVerified { .. } => {}
            EngineNetworkMsg::NewEvent(event) => unreachable!("{event:?}"),
        }
    }

    pub fn handle_liveness_message(&self, from: PeerId, bytes: Bytes) -> MessageAcceptance {
        let from = PeerIdExt::from_libp2p(&from);
        let message: LivenessMsg<_> = match ScaleCodec.decode(bytes) {
            Ok(message) => message,
            Err(error) => {
                log::error!("failed to decode liveness message from {from}: {error}");
                return MessageAcceptance::Reject;
            }
        };

        let event = match message {
            LivenessMsg::PolkaCertificate(certificate) => {
                CoreNetworkEvent::PolkaCertificate(from, certificate)
            }
            LivenessMsg::SkipRoundCertificate(certificate) => {
                CoreNetworkEvent::RoundCertificate(from, certificate)
            }
            LivenessMsg::Vote(vote) => CoreNetworkEvent::Vote(from, vote),
        };
        self.output_port.send(event);

        MessageAcceptance::Accept
    }

    pub fn handle_consensus_message(&self, from: PeerId, bytes: Bytes) -> MessageAcceptance {
        let from = PeerIdExt::from_libp2p(&from);
        let message: SignedConsensusMsg<_> = match ScaleCodec.decode(bytes) {
            Ok(message) => message,
            Err(error) => {
                log::error!("failed to decode consensus message from {from}: {error}");
                return MessageAcceptance::Reject;
            }
        };

        let event = match message {
            SignedConsensusMsg::Vote(vote) => CoreNetworkEvent::Vote(from, vote),
            SignedConsensusMsg::Proposal(proposal) => CoreNetworkEvent::Proposal(from, proposal),
        };
        self.output_port.send(event);

        MessageAcceptance::Accept
    }

    pub fn handle_proposal_part(&self, from: PeerId, bytes: Bytes) -> MessageAcceptance {
        let from = PeerIdExt::from_libp2p(&from);
        let message: StreamMessage<_> = match ScaleCodec.decode(bytes) {
            Ok(message) => message,
            Err(error) => {
                log::error!("failed to decode proposal part from {from}: {error}");
                return MessageAcceptance::Reject;
            }
        };

        self.output_port
            .send(CoreNetworkEvent::ProposalPart(from, message));

        MessageAcceptance::Accept
    }
}
