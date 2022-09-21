//! Gear node loader.
//!
//! This tool sends semi-random data to the gear node with one main purpose - crash it.
//! The sent data is not completely random as it is usually in fuzz-kind tests. The tool
//! gets properly structured data acceptable by the gear node and randomizes it's "fields".
//! That's why generated data is called semi-random.

use std::{fs::File, io::Write};

use futures::stream::StreamExt;
use rand::rngs::SmallRng;

use args::{parse_cli_params, LoadParams, Params};
use task::{generators, TaskPool};
use anyhow::Result;

mod args;
mod reporter;
mod task;
mod utils;

/// Main entry-point
#[tokio::main]
async fn main() {
    if let Err(e) = run().await {
        eprintln!("{e:}");
    }
}

async fn run() -> Result<()> {
    let params = parse_cli_params();

    match params {
        Params::Dump { seed } => dump_with_seed(seed),
        Params::Load(load_params) => load_node(load_params).await,
    }
}

// todo use anyhow?
fn dump_with_seed(seed: u64) -> Result<()> {
    let code = generators::generate_gear_program::<SmallRng>(seed);
    let mut file = File::create("out.wasm")?;
    file.write_all(&code)?;
    Ok(())
}

async fn load_node(params: LoadParams) -> Result<()> {
    let gear_api = utils::obtain_gear_api(&params.endpoint, &params.user).await?;
    let mut task_pool = TaskPool::try_new(params.workers, params.seed)?;

    task_pool.run(gear_api).await;

    for rep in task_pool.next().await {
        match rep {
            Ok(r) => r.report()?,
            Err(e) => println!("Error inside task: {e}"),
        }
    }
    Ok(())
}
