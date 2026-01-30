//! CLI args for the `ethexe-node-loader`

use anyhow::{Error, anyhow};
use clap::Parser;
use std::str::FromStr;

#[derive(Debug, Parser)]
#[clap(name = "ethexe-node-loader")]
pub enum Params {
    /// Dump the wasm program with provided seed to "out.wasm"
    Dump {
        /// Seed value used to generate program via wasm-gen
        seed: u64,
    },
    /// Perform load test on the node
    Load(LoadParams),
}

/// Parameters for the load test. Default values come from .env.example.local file.
#[derive(Debug, Parser)]
pub struct LoadParams {
    /// Ethexe node
    #[arg(long, default_value = "ws://localhost:8545")]
    pub node: String,
    #[arg(long, default_value = "ws://localhost:9944")]
    pub ethexe_node: String,

    /// Router address to send messages into.
    #[arg(long, default_value = "0xCf7Ed3AccA5a467e9e704C703E8D87F634fB0Fc9")]
    pub router_address: String,
    /// WVara proxy address to work with tokens.
    #[arg(long, default_value = "0x84eA74d481Ee0A5332c457a4d796187F6Ba67fEB")]
    pub wvara_address: String,

    /// A private key for sender account.
    #[arg(
        long,
        default_value = "0xac0974bec39a17e36ba4a6b4d238ff944bacb478cbed5efcae784d7bf4f2ff80"
    )]
    pub sender_private_key: String,
    #[arg(long, default_value = "0xf39fd6e51aad88f6f4ce6ab8827279cfffb92266")]
    pub sender_address: String,

    #[arg(long)]
    pub loader_seed: Option<u64>,
    #[arg(long)]
    pub code_seed_type: Option<SeedVariant>,
    #[arg(long, short, default_value = "1")]
    pub workers: usize,
    #[arg(long, short, default_value = "1")]
    pub batch_size: usize,
}

pub fn parse_cli_params() -> Params {
    Params::parse()
}

#[derive(Debug, Clone)]
pub enum SeedVariant {
    // TODO remove later (considering)
    Dynamic(u64),
    Constant(u64),
}

impl FromStr for SeedVariant {
    type Err = Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let s = s.to_lowercase();
        let input = s.split('=').collect::<Vec<_>>();
        if input.len() != 2 {
            return Err(anyhow!(
                "Invalid seed argument format {s:?}. Must be 'seed_variant=num'"
            ));
        }

        let variant = input[0];
        let num = input[1].parse::<u64>()?;
        match variant {
            "start" => Ok(SeedVariant::Dynamic(num)),
            "constant" => Ok(SeedVariant::Constant(num)),
            v => Err(anyhow!("Invalid variant {v}")),
        }
    }
}
