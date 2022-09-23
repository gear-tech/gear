//! Gear node loader.
//!
//! This tool sends semi-random data to the gear node with one main purpose - crash it.
//! The sent data is not completely random as it is usually in fuzz-kind tests. The tool
//! gets properly structured data acceptable by the gear node and randomizes it's "fields".
//! That's why generated data is called semi-random.

use anyhow::Result;
use args::{parse_cli_params, LoadParams, Params};
use batch_pool::{generators, BatchPool};
use rand::rngs::SmallRng;
use std::{fs::File, io::Write};

mod args;
mod batch_pool;
mod utils;

/// Main entry-point
#[tokio::main]
async fn main() {
    let params = parse_cli_params();
    if let Err(e) = run(params).await {
        eprintln!("{e:}");
        std::process::exit(1)
    }
}

async fn run(params: Params) -> Result<()> {
    match params {
        Params::Dump { seed } => dump_with_seed(seed),
        Params::Load(load_params) => load_node(load_params).await,
    }
}

fn dump_with_seed(seed: u64) -> Result<()> {
    let code = generators::generate_gear_program::<SmallRng>(seed);
    let mut file = File::create("out.wasm")?;
    file.write_all(&code)?;
    Ok(())
}

async fn load_node(params: LoadParams) -> Result<()> {
    let gear_api = utils::obtain_gear_api(&params.endpoint, &params.user).await?;
    Ok(BatchPool::<SmallRng>::new(
        params.workers,
        params.batch_size,
        params.code_seed_type,
        gear_api,
    )
    .run()
    .await)
}
