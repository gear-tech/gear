use std::collections::{BTreeMap, BTreeSet};

use async_trait::async_trait;
use bytes::Bytes;
use ethexe_malachite_core::{MalachiteCtx, ScaleCodec};
use libp2p::request_response;
use malachitebft_codec::Codec as MalachiteCodec;
use malachitebft_core_consensus::{LivenessMsg, SignedConsensusMsg};
use malachitebft_core_types::{SigningScheme, Validator, ValidatorProof};
use malachitebft_engine::{
    network::{NetworkEvent, Status},
    util::{output_port::OutputPort, streaming::StreamMessage},
};
use malachitebft_network::{Channel, Event as LaneEvent, NetworkStateDump, PeerId};
use malachitebft_sync::{self as sync, RawMessage, Request};
use ractor::{Actor, ActorProcessingErr, ActorRef, RpcReplyPort};
use tokio::sync::mpsc;

use super::{AppNetworkMsg, EngineNetworkMsg, EngineNetworkRef};

type Ctx = MalachiteCtx;

#[allow(dead_code)]
pub(crate) enum LaneCommand {
    PublishConsensus(Bytes),
    PublishLiveness(Bytes),
    PublishProposalPart(Bytes),
    BroadcastStatus(Bytes),
    OutgoingRequest {
        peer: PeerId,
        body: Bytes,
        reply: RpcReplyPort<sync::OutboundRequestId>,
    },
    OutgoingResponse {
        request_id: request_response::InboundRequestId,
        body: Bytes,
    },
    DumpState(RpcReplyPort<Option<NetworkStateDump>>),
    UpdatePersistentPeers(
        malachitebft_network::PersistentPeersOp,
        RpcReplyPort<Result<(), malachitebft_network::PersistentPeerError>>,
    ),
    UpdateValidatorSet(Vec<malachitebft_network::ValidatorInfo>),
    ValidatorProofVerified {
        peer_id: PeerId,
        result: malachitebft_network::validator_proof::ProofVerificationResult,
        public_key: Option<Vec<u8>>,
    },
}

pub(crate) struct AdapterState {
    listen_addrs: Vec<malachitebft_network::Multiaddr>,
    peers: BTreeSet<PeerId>,
    inbound_requests: BTreeMap<sync::InboundRequestId, request_response::InboundRequestId>,
    output_port: OutputPort<NetworkEvent<Ctx>>,
}

pub(crate) struct Adapter {
    lane_tx: mpsc::Sender<LaneCommand>,
    local_peer_id: PeerId,
    codec: ScaleCodec,
}

impl Adapter {
    fn new(lane_tx: mpsc::Sender<LaneCommand>, local_peer_id: PeerId) -> Self {
        Self {
            lane_tx,
            local_peer_id,
            codec: ScaleCodec,
        }
    }
}

impl Adapter {
    async fn send_lane_command(&self, command: LaneCommand) {
        if let Err(error) = self.lane_tx.send(command).await {
            log::error!("failed to send Malachite lane command: {error}");
        }
    }

    async fn publish_encoded<T>(&self, value: &T, command: impl FnOnce(Bytes) -> LaneCommand)
    where
        ScaleCodec: MalachiteCodec<T>,
    {
        match self.codec.encode(value) {
            Ok(bytes) => self.send_lane_command(command(bytes)).await,
            Err(error) => log::error!("failed to encode Malachite network message: {error}"),
        }
    }

    fn handle_lane_event(&self, event: LaneEvent, state: &mut AdapterState) {
        match event {
            LaneEvent::Listening(addr) => {
                state.listen_addrs.push(addr.clone());
                state.output_port.send(NetworkEvent::Listening(addr));
            }
            LaneEvent::PeerConnected(peer) => {
                state.peers.insert(peer);
                state.output_port.send(NetworkEvent::PeerConnected(peer));
            }
            LaneEvent::PeerDisconnected(peer) => {
                state.peers.remove(&peer);
                state.output_port.send(NetworkEvent::PeerDisconnected(peer));
            }
            LaneEvent::LivenessMessage(Channel::Liveness, from, bytes) => {
                self.handle_liveness_message(from, bytes, state);
            }
            LaneEvent::LivenessMessage(channel, from, _) => {
                log::error!("unexpected liveness message from {from} on {channel} channel");
            }
            LaneEvent::ConsensusMessage(Channel::Consensus, from, bytes) => {
                self.handle_consensus_message(from, bytes, state);
            }
            LaneEvent::ConsensusMessage(Channel::ProposalParts, from, bytes) => {
                self.handle_proposal_part(from, bytes, state);
            }
            LaneEvent::ConsensusMessage(Channel::Sync, from, bytes) => {
                self.handle_status(from, bytes, state);
            }
            LaneEvent::ConsensusMessage(channel, from, _) => {
                log::error!("unexpected consensus message from {from} on {channel} channel");
            }
            LaneEvent::Sync(message) => self.handle_sync_message(message, state),
            LaneEvent::ValidatorProofReceived {
                peer_id,
                proof_bytes,
            } => self.handle_validator_proof(peer_id, proof_bytes, state),
        }
    }

    fn handle_liveness_message(&self, from: PeerId, bytes: Bytes, state: &mut AdapterState) {
        let message: LivenessMsg<Ctx> = match self.codec.decode(bytes) {
            Ok(message) => message,
            Err(error) => {
                log::error!("failed to decode liveness message from {from}: {error}");
                return;
            }
        };

        let event = match message {
            LivenessMsg::PolkaCertificate(certificate) => {
                NetworkEvent::PolkaCertificate(from, certificate)
            }
            LivenessMsg::SkipRoundCertificate(certificate) => {
                NetworkEvent::RoundCertificate(from, certificate)
            }
            LivenessMsg::Vote(vote) => NetworkEvent::Vote(from, vote),
        };
        state.output_port.send(event);
    }

    fn handle_consensus_message(&self, from: PeerId, bytes: Bytes, state: &mut AdapterState) {
        let message: SignedConsensusMsg<Ctx> = match self.codec.decode(bytes) {
            Ok(message) => message,
            Err(error) => {
                log::error!("failed to decode consensus message from {from}: {error}");
                return;
            }
        };

        let event = match message {
            SignedConsensusMsg::Vote(vote) => NetworkEvent::Vote(from, vote),
            SignedConsensusMsg::Proposal(proposal) => NetworkEvent::Proposal(from, proposal),
        };
        state.output_port.send(event);
    }

    fn handle_proposal_part(&self, from: PeerId, bytes: Bytes, state: &mut AdapterState) {
        let message: StreamMessage<
            <MalachiteCtx as malachitebft_core_types::Context>::ProposalPart,
        > = match self.codec.decode(bytes) {
            Ok(message) => message,
            Err(error) => {
                log::error!("failed to decode proposal part from {from}: {error}");
                return;
            }
        };

        state
            .output_port
            .send(NetworkEvent::ProposalPart(from, message));
    }

    fn handle_status(&self, from: PeerId, bytes: Bytes, state: &mut AdapterState) {
        let status: sync::Status<Ctx> = match self.codec.decode(bytes) {
            Ok(status) => status,
            Err(error) => {
                log::error!("failed to decode status message from {from}: {error}");
                return;
            }
        };

        if from != status.peer_id {
            log::error!(
                "mismatched status peer id: received from {from}, payload says {}",
                status.peer_id
            );
            return;
        }

        state.output_port.send(NetworkEvent::Status(
            status.peer_id,
            Status::new(status.tip_height, status.history_min_height),
        ));
    }

    fn handle_sync_message(&self, message: RawMessage, state: &mut AdapterState) {
        match message {
            RawMessage::Request {
                request_id,
                peer,
                body,
            } => {
                let request: Request<Ctx> = match self.codec.decode(body) {
                    Ok(request) => request,
                    Err(error) => {
                        log::error!("failed to decode sync request from {peer}: {error}");
                        return;
                    }
                };

                let malachite_request_id = sync::InboundRequestId::new(request_id);
                state
                    .inbound_requests
                    .insert(malachite_request_id.clone(), request_id);
                state.output_port.send(NetworkEvent::SyncRequest(
                    malachite_request_id,
                    peer,
                    request,
                ));
            }
            RawMessage::Response {
                request_id,
                peer,
                body,
            } => {
                let response = match self.codec.decode(body) {
                    Ok(response) => Some(response),
                    Err(error) => {
                        log::error!("failed to decode sync response from {peer}: {error}");
                        None
                    }
                };

                state.output_port.send(NetworkEvent::SyncResponse(
                    sync::OutboundRequestId::new(request_id),
                    peer,
                    response,
                ));
            }
        }
    }

    fn handle_validator_proof(
        &self,
        peer_id: PeerId,
        proof_bytes: Bytes,
        state: &mut AdapterState,
    ) {
        let proof: ValidatorProof<Ctx> = match self.codec.decode(proof_bytes) {
            Ok(proof) => proof,
            Err(error) => {
                log::warn!("failed to decode validator proof from {peer_id}: {error}");
                return;
            }
        };

        if proof.peer_id != peer_id.to_bytes() {
            log::warn!("validator proof peer id does not match sender {peer_id}");
            let _ = self.lane_tx.try_send(LaneCommand::ValidatorProofVerified {
                peer_id,
                result: malachitebft_network::validator_proof::ProofVerificationResult::Invalid,
                public_key: None,
            });
            return;
        }

        state
            .output_port
            .send(NetworkEvent::ValidatorProofReceived { peer_id, proof });
    }
}

#[async_trait]
impl Actor for Adapter {
    type Msg = EngineNetworkMsg;
    type State = AdapterState;
    type Arguments = ();

    async fn pre_start(
        &self,
        _myself: ActorRef<Self::Msg>,
        _args: Self::Arguments,
    ) -> Result<Self::State, ActorProcessingErr> {
        Ok(AdapterState {
            listen_addrs: Vec::new(),
            peers: BTreeSet::new(),
            inbound_requests: BTreeMap::new(),
            output_port: OutputPort::with_capacity(128),
        })
    }

    async fn handle(
        &self,
        _myself: ActorRef<Self::Msg>,
        message: Self::Msg,
        state: &mut Self::State,
    ) -> Result<(), ActorProcessingErr> {
        match message {
            EngineNetworkMsg::Subscribe(subscriber) => {
                for addr in &state.listen_addrs {
                    subscriber.send(NetworkEvent::Listening(addr.clone()));
                }

                for peer in &state.peers {
                    subscriber.send(NetworkEvent::PeerConnected(*peer));
                }

                subscriber.subscribe_to_port(&state.output_port);
            }
            EngineNetworkMsg::PublishConsensusMsg(message) => {
                self.publish_encoded(&message, LaneCommand::PublishConsensus)
                    .await;
            }
            EngineNetworkMsg::PublishLivenessMsg(message) => {
                self.publish_encoded(&message, LaneCommand::PublishLiveness)
                    .await;
            }
            EngineNetworkMsg::PublishProposalPart(message) => {
                self.publish_encoded(&message, LaneCommand::PublishProposalPart)
                    .await;
            }
            EngineNetworkMsg::BroadcastStatus(status) => {
                let status = sync::Status {
                    peer_id: self.local_peer_id,
                    tip_height: status.tip_height,
                    history_min_height: status.history_min_height,
                };
                self.publish_encoded(&status, LaneCommand::BroadcastStatus)
                    .await;
            }
            EngineNetworkMsg::OutgoingRequest(peer, request, reply) => {
                self.publish_encoded(&request, |body| LaneCommand::OutgoingRequest {
                    peer,
                    body,
                    reply,
                })
                .await;
            }
            EngineNetworkMsg::OutgoingResponse(request_id, response) => {
                let Some(request_id) = state.inbound_requests.remove(&request_id) else {
                    log::error!("missing inbound sync request {request_id}");
                    return Ok(());
                };

                self.publish_encoded(&response, |body| LaneCommand::OutgoingResponse {
                    request_id,
                    body,
                })
                .await;
            }
            EngineNetworkMsg::DumpState(reply) => {
                if let Err(error) = self.lane_tx.send(LaneCommand::DumpState(reply)).await {
                    log::error!("failed to send Malachite dump-state command: {error}");
                    if let LaneCommand::DumpState(reply) = error.0 {
                        let _ = reply.send(None);
                    }
                }
            }
            EngineNetworkMsg::UpdatePersistentPeers(op, reply) => {
                if let Err(error) = self
                    .lane_tx
                    .send(LaneCommand::UpdatePersistentPeers(op, reply))
                    .await
                {
                    log::error!("failed to send Malachite persistent-peer command: {error}");
                    if let LaneCommand::UpdatePersistentPeers(_, reply) = error.0 {
                        let _ = reply.send(Err(
                            malachitebft_network::PersistentPeerError::NetworkStopped,
                        ));
                    }
                }
            }
            EngineNetworkMsg::UpdateValidatorSet(validators) => {
                let validators = validators
                    .iter()
                    .map(|validator| malachitebft_network::ValidatorInfo {
                        address: validator.address().to_string(),
                        public_key: <<MalachiteCtx as malachitebft_core_types::Context>::SigningScheme as SigningScheme>::encode_public_key(
                            validator.public_key(),
                        ),
                        voting_power: validator.voting_power(),
                    })
                    .collect();
                self.send_lane_command(LaneCommand::UpdateValidatorSet(validators))
                    .await;
            }
            EngineNetworkMsg::ValidatorProofVerified {
                peer_id,
                result,
                public_key,
            } => {
                self.send_lane_command(LaneCommand::ValidatorProofVerified {
                    peer_id,
                    result,
                    public_key,
                })
                .await;
            }
            EngineNetworkMsg::NewEvent(event) => self.handle_lane_event(event, state),
        }

        Ok(())
    }
}

pub struct MalachiteNetworkParts {
    network_ref: EngineNetworkRef,
    tx_network: mpsc::Sender<AppNetworkMsg>,
    events_tx: mpsc::UnboundedSender<LaneEvent>,
}

impl MalachiteNetworkParts {
    pub(crate) fn new(
        network_ref: EngineNetworkRef,
        tx_network: mpsc::Sender<AppNetworkMsg>,
        events_tx: mpsc::UnboundedSender<LaneEvent>,
    ) -> Self {
        Self {
            network_ref,
            tx_network,
            events_tx,
        }
    }

    pub(crate) fn events_tx(&self) -> mpsc::UnboundedSender<LaneEvent> {
        self.events_tx.clone()
    }

    pub fn into_engine_parts(self) -> (EngineNetworkRef, mpsc::Sender<AppNetworkMsg>) {
        (self.network_ref, self.tx_network)
    }
}

pub(crate) async fn spawn_adapter(
    lane_tx: mpsc::Sender<LaneCommand>,
    local_peer_id: PeerId,
) -> anyhow::Result<MalachiteNetworkParts> {
    let adapter = Adapter::new(lane_tx, local_peer_id);
    let (network_ref, _) = Actor::spawn(None, adapter, ()).await?;

    let (events_tx, mut events_rx) = mpsc::unbounded_channel::<LaneEvent>();
    tokio::spawn({
        let network_ref = network_ref.clone();
        async move {
            while let Some(event) = events_rx.recv().await {
                if let Err(error) = network_ref.cast(EngineNetworkMsg::NewEvent(event)) {
                    log::error!("failed to send Malachite network event to adapter: {error}");
                    break;
                }
            }
        }
    });

    let (tx_network, mut rx_network) = mpsc::channel::<AppNetworkMsg>(128);
    tokio::spawn({
        let network_ref = network_ref.clone();
        async move {
            while let Some(message) = rx_network.recv().await {
                if let Err(error) = network_ref.cast(message.into()) {
                    log::error!("failed to send Malachite network message to adapter: {error}");
                    break;
                }
            }
        }
    });

    Ok(MalachiteNetworkParts::new(
        network_ref,
        tx_network,
        events_tx,
    ))
}
