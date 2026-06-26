use super::adapter::LaneCommand;
use libp2p::{Multiaddr, PeerId, request_response};
use malachitebft_network::{
    LocalNodeInfo, NetworkStateDump, PersistentPeerError, PersistentPeersOp, ValidatorInfo,
};
use malachitebft_sync::{OutboundRequestId, ResponseChannel};
use std::collections::{HashMap, HashSet};
use tokio::sync::mpsc;

pub(crate) struct State {
    pub lane_rx: mpsc::Receiver<LaneCommand>,
    pub events_tx: mpsc::UnboundedSender<malachitebft_network::Event>,
    pub persistent_peers: HashSet<Multiaddr>,
    pub validators: Vec<ValidatorInfo>,
    pub inbound_sync_requests: HashMap<request_response::InboundRequestId, ResponseChannel>,
    pub outbound_sync_requests: HashMap<request_response::OutboundRequestId, OutboundRequestId>,
    pub verified_validator_proofs: HashMap<PeerId, Vec<u8>>,
}

impl State {
    pub(crate) fn new(
        lane_rx: mpsc::Receiver<LaneCommand>,
        events_tx: mpsc::UnboundedSender<malachitebft_network::Event>,
        persistent_peers: Vec<Multiaddr>,
    ) -> Self {
        Self {
            lane_rx,
            events_tx,
            persistent_peers: persistent_peers.into_iter().collect(),
            validators: Vec::new(),
            inbound_sync_requests: HashMap::new(),
            outbound_sync_requests: HashMap::new(),
            verified_validator_proofs: HashMap::new(),
        }
    }

    pub(crate) fn apply_persistent_peer_op(
        &mut self,
        op: PersistentPeersOp,
    ) -> Result<(), PersistentPeerError> {
        match op {
            PersistentPeersOp::Add(addr) => self
                .persistent_peers
                .insert(addr)
                .then_some(())
                .ok_or(PersistentPeerError::AlreadyExists),
            PersistentPeersOp::Remove(addr) => self
                .persistent_peers
                .remove(&addr)
                .then_some(())
                .ok_or(PersistentPeerError::NotFound),
        }
    }

    pub(crate) fn dump_state(&self, local_peer_id: PeerId) -> NetworkStateDump {
        NetworkStateDump {
            local_node: LocalNodeInfo {
                moniker: String::new(),
                peer_id: local_peer_id,
                listen_addr: Multiaddr::empty(),
                consensus_address: None,
                proof_bytes: None,
                is_validator: false,
                persistent_peers_only: false,
                subscribed_topics: HashSet::new(),
            },
            peers: HashMap::new(),
            validator_set: self.validators.clone(),
            persistent_peer_ids: Vec::new(),
            persistent_peer_addrs: self.persistent_peers.iter().cloned().collect(),
        }
    }
}
