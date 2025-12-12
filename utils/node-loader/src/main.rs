//! Gear node loader.
//!
//! This tool sends semi-random data to the gear node with one main purpose - crash it.
//! The sent data is not completely random as it is usually in fuzz-kind tests. The tool
//! gets properly structured data acceptable by the gear node and randomizes it's "fields".
//! That's why generated data is called semi-random.

use anyhow::{Error, Result, anyhow};
use args::{LoadParams, Params, parse_cli_params};
use batch_pool::{BatchPool, api::GearApiFacade};
use futures::prelude::*;
use gsdk::blocks::Block;
use names::Generator;
use rand::rngs::SmallRng;
use tokio::sync::broadcast::Sender;

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

/// Connect to API of provided node address and subscribe for events.
///
/// Broadcast new events to receivers.
async fn listen_blocks(tx: Sender<Block>, node: String) -> Result<()> {
    let api = gsdk::Api::new(node.as_str()).await?;
    api.blocks()
        .subscribe_finalized()
        .await?
        .map_err(Error::from)
        .try_for_each(|block| {
            future::ready(
                tx.send(block)
                    .map_err(|_| anyhow!("failed to send block"))
                    .map(|_| ()),
            )
        })
        .await?;

    Err(anyhow!("Listen events: Can't get new events"))
}

fn parse_name(name: &str) -> (&str, Option<&str>) {
    name.split_once(':')
        .map_or((name, None), |(suri, passwd)| (suri, Some(passwd)))
}

async fn load_node(params: LoadParams) -> Result<()> {
    let (tx, rx) = tokio::sync::broadcast::channel(16);
    let mut name_gen = Generator::default();
    let run_name = name_gen
        .next()
        .ok_or(anyhow!("Failed generating run name"))?;

    // this should not be dropped, until the loader works
    let _guard = log::init_log(run_name.clone())?;

    tracing::info!(
        "Running {} of version {}. Run name: {run_name}.",
        env!("CARGO_PKG_NAME"),
        env!("CARGO_PKG_VERSION"),
    );

    let (suri, passwd) = parse_name(&params.user);
    let api = GearApiFacade::try_new(&params.node, suri, passwd).await?;

    let batch_pool =
        BatchPool::<SmallRng>::new(api, params.batch_size, params.workers, rx.resubscribe());

    let run_result = tokio::select! {
        r = listen_blocks(tx, params.node.clone()) => r,
        r = batch_pool.run(params, rx) => r,
    };

    run_result
}
