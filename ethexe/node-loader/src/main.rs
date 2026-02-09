mod args;
mod batch;
mod utils;
use alloy::{
    network::Network,
    primitives::Address,
    providers::{Provider, RootProvider},
    signers::local::{MnemonicBuilder, coins_bip39::English},
};
use anyhow::Result;
use args::{Params, parse_cli_params};
use ethexe_ethereum::Ethereum;

use rand::rngs::SmallRng;
use std::str::FromStr;
use tokio::{sync::broadcast::Sender, task::JoinSet};
use tracing::info;

use crate::{args::LoadParams, batch::BatchPool};

/// Entrypoint for the node-loader CLI.
///
/// Runs a continuous load test against an `ethexe` dev node, generating randomized batches
/// that upload code/programs, send messages, send replies, and claim values to stress-test
/// the runtime and networking stack.
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

/// Default Hardhat/Anvil mnemonic.
const MNEMONIC: &str = "test test test test test test test test test test test junk";

/// Derive a `gsigner::secp256k1::Signer` (with one imported key) from the
/// standard derivation index `m/44'/60'/0'/0/{index}`.
///
/// Returns the signer together with the corresponding gsigner address.
fn derive_signer(index: u32) -> Result<(gsigner::secp256k1::Signer, gsigner::secp256k1::Address)> {
    // Derive the raw k256 key via alloy's BIP-32/BIP-39 MnemonicBuilder.
    let alloy_signer = MnemonicBuilder::<English>::default()
        .phrase(MNEMONIC)
        .index(index)
        .map_err(|e| anyhow::anyhow!("bad derivation index {index}: {e}"))?
        .build()
        .map_err(|e| anyhow::anyhow!("mnemonic derivation failed at index {index}: {e}"))?;

    // Extract the 32-byte secret and import it into a gsigner in-memory signer.
    let seed: [u8; 32] = alloy_signer.to_bytes().0;
    let private_key = gsigner::secp256k1::PrivateKey::from_seed(seed)?;
    let signer = gsigner::secp256k1::Signer::memory();
    let pubkey = signer.import(private_key)?;
    let address = pubkey.to_address();

    // Sanity-check: alloy and gsigner must agree on the address.
    let alloy_addr = alloy_signer.address();
    anyhow::ensure!(
        address.0 == alloy_addr.0.0,
        "address mismatch at index {index}: gsigner={address:?}, alloy={alloy_addr:?}",
    );

    Ok((signer, address))
}

/// Create a gsigner from a hex-encoded private key string.
/// Accepts keys with or without "0x" prefix.
fn signer_from_private_key(
    private_key_hex: &str,
) -> Result<(gsigner::secp256k1::Signer, gsigner::secp256k1::Address)> {
    let hex_str = private_key_hex
        .strip_prefix("0x")
        .unwrap_or(private_key_hex);
    let seed_bytes =
        alloy::hex::decode(hex_str).map_err(|e| anyhow::anyhow!("invalid hex private key: {e}"))?;

    if seed_bytes.len() != 32 {
        return Err(anyhow::anyhow!(
            "private key must be 32 bytes, got {}",
            seed_bytes.len()
        ));
    }

    let mut seed = [0u8; 32];
    seed.copy_from_slice(&seed_bytes);

    let private_key = gsigner::secp256k1::PrivateKey::from_seed(seed)?;
    let signer = gsigner::secp256k1::Signer::memory();
    let pubkey = signer.import(private_key)?;
    let address = pubkey.to_address();

    Ok((signer, address))
}

async fn load_node(params: LoadParams) -> Result<()> {
    const MAX_WORKERS: usize = 48;
    const MINT_AMOUNT: u128 = 500_000_000_000_000_000_000;
    // Anvil mnemonic index 0 is the deployer.
    const DEPLOYER_INDEX: u32 = 0;

    const WORKER_START_INDEX: u32 = 2;

    if params.workers == 0 {
        return Err(anyhow::anyhow!("workers must be greater than 0"));
    }

    if params.workers > MAX_WORKERS {
        return Err(anyhow::anyhow!("workers must not exceed {MAX_WORKERS}"));
    }

    let router_addr = Address::from_str(&params.router_address)?;

    // Use sender private key if provided, otherwise default to Anvil deployer (index 0)
    let (deployer_signer, deployer_address) =
        if let Some(ref private_key) = params.sender_private_key {
            info!("Using provided sender private key");
            signer_from_private_key(private_key)?
        } else {
            info!(
                "Using default Anvil deployer (derivation index {})",
                DEPLOYER_INDEX
            );
            derive_signer(DEPLOYER_INDEX)?
        };

    info!(
        "deployer address: 0x{}",
        alloy::hex::encode(deployer_address.0)
    );

    let deployer_api = Ethereum::new(
        &params.node,
        router_addr.into(),
        deployer_signer,
        deployer_address,
    )
    .await?;

    // Derive worker accounts (indices 2 .. 2+workers) concurrently so we
    // don't block on each Ethereum handshake during the loop.
    let mut init_tasks: JoinSet<Result<(u32, u32, gsigner::secp256k1::Address, Ethereum)>> =
        JoinSet::new();
    for i in 0..params.workers as u32 {
        let index = WORKER_START_INDEX + i;
        let (signer, address) = derive_signer(index)?;
        let node = params.node.clone();
        let router = router_addr;

        init_tasks.spawn(async move {
            let api = Ethereum::new(&node, router.into(), signer, address).await?;
            Ok((i, index, address, api))
        });
    }

    let mut apis = Vec::with_capacity(params.workers);
    let mut worker_addrs = Vec::with_capacity(params.workers);
    while let Some(result) = init_tasks.join_next().await {
        let (i, index, address, api) = result??;
        info!(
            "worker {i} (derivation index {index}): 0x{}",
            alloy::hex::encode(address.0)
        );
        worker_addrs.push(address);
        apis.push(api);
    }

    // Fund and approve workers after all handshakes are done.
    for (address, api) in worker_addrs.iter().zip(apis.iter()) {
        deployer_api
            .wrapped_vara()
            .mint((*address).into(), MINT_AMOUNT)
            .await?;
        api.wrapped_vara().approve_all((*address).into()).await?;
    }

    let provider = apis
        .first()
        .expect("workers must be greater than 0")
        .provider()
        .clone();

    // proportionally increase the channel size to workers and batch size
    // so that we can keep up with the load.
    let (tx, rx) = tokio::sync::broadcast::channel(params.batch_size * params.workers * 48);

    let batch_pool = BatchPool::<SmallRng>::new(
        apis,
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
