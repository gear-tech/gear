//! CLI args for the `gear-node-loader`

use anyhow::{Error, anyhow};
use clap::Parser;
use std::str::FromStr;

#[derive(Debug, Parser)]
#[clap(name = "node-loader")]
pub enum Params {
    /// Dump the wasm program with provided seed to "out.wasm"
    Dump {
        /// Seed value used to generate program via wasm-gen
        seed: u64,
    },
    /// Perform load test on the node
    Load(LoadParams),
}

#[derive(Debug, Parser)]
pub struct LoadParams {
    #[arg(long, default_value = "ws://localhost:9944")]
    pub node: String,

    /// Node stopping service.
    #[arg(long, default_value = "http://localhost:5000/executions/start")]
    pub node_stopper: String,

    /// User name
    #[arg(long, default_value = "//Bob")]
    pub user: String,

    /// Root account of the node
    #[arg(long, default_value = "//Alice")]
    pub root: String,

    /// Starting seed for loading the network
    #[arg(long)]
    pub loader_seed: Option<u64>,

    /// Seed used to generate random seeds for various internal generators.
    /// If the parameter isn't provided, then timestamp will be used by default.
    /// There are either 2 seed variants: start or constant. Start sets starting
    ///  seed value for the generator. Constant sets constant seed which will be
    /// used in every test, therefore generated input data (for example, program)
    /// for each test will be the same.
    /// Example value: `<seed_variant>=<seed_u64_value>`.
    #[arg(long)]
    pub code_seed_type: Option<SeedVariant>,

    /// Desirable amount of workers in task pool.
    #[arg(long, short, default_value = "8")]
    pub workers: usize,

    /// Desirable amount of calls in the sending batch.
    #[arg(long, short, default_value = "4")]
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
