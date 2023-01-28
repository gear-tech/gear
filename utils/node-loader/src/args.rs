//! CLI args for the `gear-node-loader`

use anyhow::Error;
use std::str::FromStr;
use structopt::StructOpt;

#[derive(Debug, StructOpt)]
#[structopt(name = "node-loader")]
pub enum Params {
    /// Dump the wasm program with provided seed to "out.wasm"
    Dump {
        /// Seed value used to generate program via wasm-gen
        seed: u64,
    },
    /// Perform load test on the node
    Load(LoadParams),
}

#[derive(Debug, StructOpt)]
pub struct LoadParams {
    #[structopt(long, default_value = "ws://localhost:9944")]
    pub node: String,

    /// Node stopping service.
    #[structopt(long, default_value = "http://localhost:5000/executions/start")]
    pub node_stopper: String,

    /// User name
    #[structopt(long, default_value = "//Bob")]
    pub user: String,

    /// Root account of the node
    #[structopt(long, default_value = "//Alice")]
    pub root: String,

    /// Starting seed for loading the network
    #[structopt(long)]
    pub loader_seed: Option<u64>,

    /// Seed used to generate random seeds for various internal generators.
    /// If the parameter isn't provided, then timestamp will be used by default.
    /// There are either 2 seed variants: start or constant. Start sets starting
    ///  seed value for the generator. Constant sets constant seed which will be
    /// used in every test, therefore generated input data (for example, program)
    /// for each test will be the same.
    /// Example value: `<seed_variant>=<seed_u64_value>`.
    #[structopt(long)]
    pub code_seed_type: Option<SeedVariant>,

    /// Desirable amount of workers in task pool.
    #[structopt(long, short, default_value = "8")]
    pub workers: usize,

    /// Desirable amount of calls in the sending batch.
    #[structopt(long, short, default_value = "4")]
    pub batch_size: usize,
}

pub fn parse_cli_params() -> Params {
    Params::from_args()
}

#[derive(Debug)]
pub enum SeedVariant {
    // TODO remove later (considering )
    Dynamic(u64),
    Constant(u64),
}

impl FromStr for SeedVariant {
    type Err = Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let s = s.to_lowercase();
        let input = s.split('=').collect::<Vec<_>>();
        if input.len() != 2 {
            return Err(anyhow::anyhow!(
                "Invalid seed argument format {s:?}. Must be 'seed_variant=num'"
            ));
        }

        let variant = input[0];
        let num = input[1].parse::<u64>()?;
        match variant {
            "start" => Ok(SeedVariant::Dynamic(num)),
            "constant" => Ok(SeedVariant::Constant(num)),
            v => Err(anyhow::anyhow!("Invalid variant {v}")),
        }
    }
}
