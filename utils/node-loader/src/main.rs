//! Gear node loader.
//!
//! This tool sends semi-random data to the gear node with one main purpose - crash it.
//! The sent data is not completely random as it is usually in fuzz-kind tests. The tool
//! gets properly structured data acceptable by the gear node and randomizes it's "fields".
//! That's why generated data is called semi-random.

use anyhow::{anyhow, Result};
use args::{parse_cli_params, LoadParams, Params};
use batch_pool::{api::GearApiFacade, BatchPool};
use gsdk::config::GearConfig;
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

async fn listen_events(tx: Sender<subxt::events::Events<GearConfig>>, node: String) -> Result<()> {
    let api = gsdk::Api::new(Some(&node)).await?;
    let mut event_listener = api.finalized_blocks().await?;

    loop {
        while let Some(events) = event_listener.next_events().await {
            tx.send(events?)?;
        }
        break Err(anyhow!("Can't get new events"));
    }
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

    let api = GearApiFacade::try_new(params.node.clone(), params.user.clone()).await?;

    let batch_pool =
        BatchPool::<SmallRng>::new(api, params.batch_size, params.workers, rx.resubscribe());

    let run_result = tokio::select! {
        r = tokio::spawn(listen_events(tx, params.node.clone())) => r?,
        r = tokio::spawn(batch_pool.run(params, rx)) => r?,
    };

    run_result
}
