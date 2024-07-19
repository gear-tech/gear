use anyhow::Result;
use ethexe_network::config::Multiaddr;
use serde::Deserialize;
use std::{fs, path::Path};

#[derive(Default, Debug, Deserialize)]
pub struct ChainSpec {
    /// Address of Ethereum Router contract.
    pub ethereum_router_address: String,

    /// Addresses of Boot Nodes.
    pub bootnodes: Vec<Multiaddr>,
}

pub fn testnet_config() -> ChainSpec {
    ChainSpec {
        ethereum_router_address: "0xf90d2956B6F36194fbac181Fb95b2f67274821da".to_string(),
        bootnodes: vec![
            "/ip4/54.183.94.171/udp/20333/quic-v1/p2p/12D3KooWQ5kJQs2WK5kzmBMShCNidpySDuLjt7aqZoVimdyCPRDz".parse().unwrap(),
            "/ip4/54.183.94.171/udp/20334/quic-v1/p2p/12D3KooWAivseD2rweVeS2fyNuVFP1hWZ2gXRtMzrNK1ci4mmoMj".parse().unwrap()
        ],
    }
}

pub fn from_file<P>(path: P) -> Result<ChainSpec>
where
    P: AsRef<Path>,
{
    let str = fs::read_to_string(path)?;
    let chain_spec: ChainSpec = toml::from_str(&str)?;

    Ok(chain_spec)
}
