//! Gear node loader.
//!
//! This tool sends semi-random data to the gear node with one main purpose - crash it.
//! The sent data is not completely random as it is usually in fuzz-kind tests. The tool
//! gets properly structured data acceptable by the gear node and randomizes it's "fields".
//! That's why generated data is called semi-random.

use anyhow::{anyhow, Result};
use args::{parse_cli_params, LoadParams, Params};
use batch_pool::{generators, BatchPool};
use gclient::GearApi;
use rand::rngs::SmallRng;
use tracing::{Instrument, Metadata};

mod args;
mod batch_pool;
mod utils;

/// Main entry-point
#[tokio::main]
async fn main() -> Result<()> {
    let params = parse_cli_params();

    run(params).await
}

async fn run(params: Params) -> Result<()> {
    match params {
        Params::Dump { seed } => utils::dump_with_seed(seed).map_err(|e| e.into()),
        Params::Load(load_params) => load_node(load_params).await,
    }
}

async fn load_node(params: LoadParams) -> Result<()> {
    let api = GearApi::init(utils::str_to_wsaddr(params.endpoint)).await?;
    tracing_subscriber::fmt()
        .pretty()
        .with_env_filter("gear_node_loader=debug")
        .try_init()
        .map_err(|_| anyhow!("Can't initialize logger"))?;

    BatchPool::<SmallRng>::new(api, params.workers, params.batch_size)
        .run(params.code_seed_type)
        .await
        .map_err(|e| e.into())
}
