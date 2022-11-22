//! Gear node loader.
//!
//! This tool sends semi-random data to the gear node with one main purpose - crash it.
//! The sent data is not completely random as it is usually in fuzz-kind tests. The tool
//! gets properly structured data acceptable by the gear node and randomizes it's "fields".
//! That's why generated data is called semi-random.

use anyhow::Result;
use args::{parse_cli_params, LoadParams, Params};
use batch_pool::BatchPool;
use rand::rngs::SmallRng;

mod args;
mod batch_pool;
mod log;
mod utils;

/// Main entry-point
#[tokio::main]
async fn main() -> Result<()> {
    let params = parse_cli_params();

    run(params).await
}

async fn run(params: Params) -> Result<()> {
    match params {
        Params::Dump { seed } => utils::dump_with_seed(seed),
        Params::Load(load_params) => load_node(load_params).await,
    }
}

async fn load_node(params: LoadParams) -> Result<()> {
    // this should not be dropped, until the loader works
    let _guard = log::init_log()?;

    tracing::info!(
        "Running {} of version: {}",
        env!("CARGO_PKG_NAME"),
        env!("CARGO_PKG_VERSION")
    );

    BatchPool::<SmallRng>::run(params).await
}
