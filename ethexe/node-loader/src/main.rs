mod args;
mod batch;
mod utils;
use alloy::{
    network::Network,
    primitives::Address,
    providers::{Provider, RootProvider},
};
use anyhow::Result;
use args::{Params, parse_cli_params};
use ethexe_common::k256::ecdsa::SigningKey;
use ethexe_ethereum::Ethereum;

use ethexe_signer::{KeyStorage, MemoryKeyStorage};
use rand::rngs::SmallRng;
use std::str::FromStr;
use tokio::sync::broadcast::Sender;
use tracing::info;

use crate::{args::LoadParams, batch::BatchPool};

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt::init();

    let params = parse_cli_params();

    match params {
        Params::Dump { seed } => {
            info!("Dump requested with seed: {}", seed);
            // Dump logic would go here
            Ok(())
        }
        Params::Load(load_params) => {
            info!("Starting load test on {}", load_params.node);

            load_node(load_params).await
        }
    }
}

async fn load_node(params: LoadParams) -> Result<()> {
    let signing_key = alloy::hex::decode(&params.sender_private_key).unwrap();

    let mut keystore = MemoryKeyStorage::empty();
    let signing_key = SigningKey::from_slice(signing_key.as_ref()).expect("Invalid signing key");
    let pubkey = keystore.add_key(signing_key.into()).unwrap();
    let signer = ethexe_signer::Signer::new(keystore);
    let router_addr = Address::from_str(&params.router_address).unwrap();

    let api = Ethereum::new(
        &params.node,
        router_addr.into(),
        signer,
        pubkey.to_address(),
    )
    .await?;
    api.wrapped_vara()
        .mint(pubkey.to_address(), 500_000_000_000_000_000_000)
        .await?;
    api.wrapped_vara().approve_all(pubkey.to_address()).await?;

    let provider = api.provider().clone();

    // proportionally increase the channel size to workers and batch size
    // so that we can keep up with the load.
    let (tx, rx) = tokio::sync::broadcast::channel(params.batch_size * params.workers * 48);

    let batch_pool = BatchPool::<SmallRng>::new(
        api,
        params.ethexe_node.clone(),
        params.workers,
        params.batch_size,
        rx.resubscribe(),
    );

    let run_result = tokio::select! {
        r = listen_blocks(tx, provider.root().clone()) => r,
        r = batch_pool.run(params, rx) => r,
    };

    run_result
}

async fn listen_blocks(
    tx: Sender<<alloy::network::Ethereum as Network>::HeaderResponse>,
    provider: RootProvider,
) -> Result<()> {
    let mut sub = provider.subscribe_blocks().await?;

    while let Ok(block) = sub.recv().await {
        tx.send(block)
            .map_err(|_| anyhow::anyhow!("Failed to send block"))?;
    }

    todo!()
}
