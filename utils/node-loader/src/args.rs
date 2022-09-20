//! CLI args for the `gear-node-loader`

use anyhow::Error;
use std::str::FromStr;
use structopt::StructOpt;

#[derive(Debug, StructOpt)]
#[structopt(name = "node-loader")]
pub(crate) enum Params {
    /// Dump the wasm program with provided seed to "out.wasm"
    Dump {
        /// Seed value used to generate program via wasm-gen
        seed: u64,
    },
    /// Perform load test on the node
    Load(LoadParams),
}

#[derive(Debug, StructOpt)]
pub(crate) struct LoadParams {
    #[structopt(long, default_value = "ws://localhost:9944")]
    pub(crate) endpoint: String,

    /// User name
    #[structopt(long, default_value = "//Alice")]
    pub(crate) user: String,

    /// Seed used to generate random seeds for various internal generators.
    /// If the parameter isn't provided, then timestamp will be used by default.
    /// There are either 2 seed variants: start or constant. Start sets starting
    ///  seed value for the generator. Constant sets constant seed which will be
    /// used in every test, therefore generated input data (for example, program)
    /// for each test will be the same.
    /// Example value: `<seed_variant>=<seed_u64_value>`.
    #[structopt(long, short)]
    pub(crate) seed: Option<SeedVariant>,

    /// Desirable amount of workers in task pool.
    #[structopt(long, short, default_value = "1")]
    pub(crate) workers: usize,
}

pub(crate) fn parse_cli_params() -> Params {
    Params::from_args()
}

#[derive(Debug)]
pub(crate) enum SeedVariant {
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
