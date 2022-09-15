//! CLI args for the `gear-node-loader`

use structopt::StructOpt;

#[derive(Debug, StructOpt)]
#[structopt(name = "node-hacker")]
pub(crate) struct Params {
    /// rpc node addr
    #[structopt(long, default_value = "ws://localhost:9944")]
    pub endpoint: String,

    /// user name
    #[structopt(long, default_value = "//Alice")]
    pub user: String,

    /// seed for random seeds generation
    #[structopt(long, short, default_value = "0")]
    pub seed: u64,

    /// dump wasm into "out.wasm" for seed and stop work
    #[structopt(long)]
    pub dump_seed: Option<u64>,

    /// generate program for seed and test it in inf loop
    #[structopt(long)]
    pub only_seed: Option<u64>,
}

pub(crate) fn parse_cli_params() -> Params {
    Params::from_args()
}