//! Gear node loader.
//!
//! This tool sends semi-random data to the gear node with one main purpose - crash it.
//! The sent data is not completely random as it is usually in fuzz-kind tests. The tool
//! gets properly structured data acceptable by the gear node and randomizes it's "fields".
//! That's why generated data is called semi-random.

use std::{fs::File, io::Write};

use rand::rngs::SmallRng;

use args::{parse_cli_params, LoadParams, Params};
use task::{generators, TaskPool};

use utils::Rng;

mod args;
mod reporter;
mod task;
mod utils;

/// Main entry-point
#[tokio::main]
async fn main() {
    if let Err(e) = run().await {
        eprintln!("{e:}");
        std::process::exit(1);
    }
}

async fn run() -> Result<(), String> {
    let params = parse_cli_params();

    match params {
        Params::Dump { seed } => dump_with_seed(seed),
        Params::Load(load_params) => load_node(load_params).await,
    }
}

// todo use anyhow?
fn dump_with_seed(seed: u64) -> Result<(), String> {
    let code = generators::generate_gear_program::<SmallRng>(seed);
    let mut file = File::create("out.wasm").map_err(|e| e.to_string())?;
    file.write_all(&code).map_err(|e| e.to_string())?;
    Ok(())
}

async fn load_node(params: LoadParams) -> Result<(), String> {
    let gear_api = utils::obtain_gear_api(&params.endpoint, &params.user).await?;
    let mut task_pool = TaskPool::<SmallRng>::try_new(params.workers, params.seed, gear_api)?;

    loop {
        let reporters = task_pool
            .run()
            .await
            .map_err(|_| format!("Task pool run failed"))?;

        for r in reporters {
            r.report()?;
        }
    }
}
