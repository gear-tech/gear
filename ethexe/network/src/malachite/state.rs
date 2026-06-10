use std::collections::{HashMap, HashSet};

use libp2p::{Multiaddr, PeerId, request_response};
use malachitebft_network::{
    LocalNodeInfo, NetworkStateDump, PersistentPeerError, PersistentPeersOp, ValidatorInfo,
};
use malachitebft_sync::{OutboundRequestId, ResponseChannel};

#[derive(Default, Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct DebugCounters {
    pub consensus_published: u64,
    pub liveness_published: u64,
    pub proposal_parts_published: u64,
    pub sync_requests_sent: u64,
    pub sync_responses_sent: u64,
    pub validator_proofs_verified: u64,
}

pub(crate) struct State {
    pub persistent_peers: HashSet<Multiaddr>,
    pub validators: Vec<ValidatorInfo>,
    pub inbound_sync_requests: HashMap<request_response::InboundRequestId, ResponseChannel>,
    pub outbound_sync_requests: HashMap<request_response::OutboundRequestId, OutboundRequestId>,
    pub verified_validator_proofs: HashMap<PeerId, Vec<u8>>,
    pub debug_counters: DebugCounters,
}

impl State {
    pub(crate) fn new(persistent_peers: Vec<Multiaddr>) -> Self {
        Self {
            persistent_peers: persistent_peers.into_iter().collect(),
            validators: Vec::new(),
            inbound_sync_requests: HashMap::new(),
            outbound_sync_requests: HashMap::new(),
            verified_validator_proofs: HashMap::new(),
            debug_counters: DebugCounters::default(),
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
