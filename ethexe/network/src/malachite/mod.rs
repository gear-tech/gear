pub(crate) mod adapter;
pub(crate) mod behaviour;
pub(crate) mod state;

#[cfg(test)]
mod tests;

pub use adapter::MalachiteNetworkParts;

use ethexe_common::injected;
use malachitebft_network::{ChannelNames, ProtocolNames};

pub type AppNetworkMsg = ethexe_malachite_core::NetworkMsg<ethexe_malachite_core::MalachiteCtx>;
pub type EngineNetworkRef = ethexe_malachite_core::NetworkRef<ethexe_malachite_core::MalachiteCtx>;
pub type EngineNetworkMsg =
    ethexe_malachite_core::EngineNetworkMsg<ethexe_malachite_core::MalachiteCtx>;

#[derive(Debug)]
pub(crate) struct Config {
    pub channel_names: ChannelNames,
    pub protocol_names: ProtocolNames,
    pub pubsub_max_size: u64,
    pub rpc_max_size: u64,
}

impl Default for Config {
    fn default() -> Self {
        let malachitebft_app_channel::app::config::P2pConfig {
            listen_addr: _,
            persistent_peers: _,
            persistent_peers_only: _,
            discovery: _,
            protocol: _, // we force gossipsub
            pubsub_max_size,
            rpc_max_size,
            protocol_names: _,
        } = malachitebft_app_channel::app::config::P2pConfig::default();

        // make sure injected transactions limits fits in max transmit size
        assert!(pubsub_max_size.as_u64() >= injected::MAX_INJECTED_TRANSACTIONS_SIZE_PER_MB as u64);

        Self {
            channel_names: ChannelNames::default(),
            protocol_names: ProtocolNames::default(),
            pubsub_max_size: pubsub_max_size.as_u64(),
            rpc_max_size: rpc_max_size.as_u64(),
        }
    }
}
