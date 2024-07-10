use anyhow::Result;
use hypercore_network::config::Multiaddr;
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
        ethereum_router_address: "0x05069E9045Ca0D2B72840c6A21C7bE588E02089A".to_string(),
        bootnodes: vec![],
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
