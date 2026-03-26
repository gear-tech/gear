use crate::{abi::deploy_send_message_multicall, args::LoadParams, batch::BatchPool};
use alloy::{hex, primitives::Address, providers::Provider};
use anyhow::{Result, anyhow};
use args::{Params, parse_cli_params};
use ethexe_ethereum::Ethereum;
use rand::rngs::SmallRng;
use std::str::FromStr;
use tokio::task::JoinSet;
use tracing::info;

mod abi;
mod args;
mod batch;
mod fuzz;
mod utils;

/// Entrypoint for the node-loader CLI.
///
/// Runs a continuous load test against an `ethexe` dev node, generating randomized batches
/// that upload code/programs, send messages, send replies, and claim values to stress-test
/// the runtime and networking stack.
#[tokio::main]
async fn main() -> Result<()> {
    let fmt = tracing_subscriber::fmt::format().with_ansi(false).compact();
    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .event_format(fmt)
        .init();

    let params = parse_cli_params();

    match params {
        Params::Dump { seed } => {
            info!("Dump requested with seed: {}", seed);

            utils::dump_with_seed(seed).await
        }
        Params::Load(load_params) => {
            info!("Starting load test on {}", load_params.node);

            load_node(load_params).await
        }
        Params::Fuzz(fuzz_params) => {
            info!("Starting syscall fuzz test on {}", fuzz_params.node);

            fuzz::run_fuzz(fuzz_params).await
        }
    }
}

async fn load_node(params: LoadParams) -> Result<()> {
    const MINT_AMOUNT: u128 = 500_000_000_000_000_000_000_000;

    if params.workers == 0 {
        return Err(anyhow!("workers must be greater than 0"));
    }

    utils::validate_worker_count(params.ethexe_nodes.len(), params.workers)?;

    for arg in params.ethexe_nodes.iter() {
        match url::Url::parse(arg) {
            Ok(_) => (),
            Err(e) => {
                return Err(anyhow!("invalid Ethexe node URL '{arg}': {e}"));
            }
        }
    }

    let router_addr = Address::from_str(&params.router_address)?;

    // Use sender private key if provided, otherwise use the default Anvil deployer account.
    let (deployer_signer, deployer_address) =
        if let Some(ref private_key) = params.sender_private_key {
            info!("Using provided sender private key");
            utils::signer_from_private_key(private_key)?
        } else {
            utils::signer_from_private_key(utils::DEPLOYER_ACCOUNT.private_key)?
        };

    info!("deployer address: 0x{}", hex::encode(deployer_address.0));

    let deployer_api = Ethereum::new(
        &params.node,
        router_addr.into(),
        deployer_signer,
        deployer_address,
    )
    .await?;

    let send_message_multicall = deploy_send_message_multicall(&deployer_api).await?;
    info!(
        "send-message multicall deployed at 0x{}",
        hex::encode(send_message_multicall.0)
    );

    // Load worker accounts from the standard Anvil mnemonic after the deployer and the current
    // validator set, so workers do not overlap with validator accounts.
    let mut init_tasks: JoinSet<Result<(u32, u32, gsigner::secp256k1::Address, Ethereum)>> =
        JoinSet::new();
    let worker_account_start = utils::worker_account_start(params.ethexe_nodes.len())?;
    for i in 0..params.workers as u32 {
        let account_index = worker_account_start + i;
        let (signer, address) = utils::signer_from_anvil_account(account_index)?;
        let node = params.node.clone();
        let router = router_addr;

        init_tasks.spawn(async move {
            let api = Ethereum::new(&node, router.into(), signer, address).await?;
            Ok((i, account_index, address, api))
        });
    }

    let mut apis = Vec::with_capacity(params.workers);
    let mut worker_addrs = Vec::with_capacity(params.workers);
    while let Some(result) = init_tasks.join_next().await {
        let (i, account_index, address, api) = result??;
        info!(
            "worker {i} (anvil account #{account_index}): 0x{}",
            hex::encode(address.0)
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
        tracing::debug!(
            "Minted {} WVARA to 0x{}",
            MINT_AMOUNT,
            hex::encode(address.0)
        );
        api.wrapped_vara().approve_all((*address).into()).await?;
        tracing::debug!("Approved all WVARA for 0x{}", hex::encode(address.0));

        // Approve multicall contract to spend wVARA on behalf of this worker.
        api.wrapped_vara()
            .approve_all(send_message_multicall.into())
            .await?;
        tracing::debug!(
            "Approved all WVARA for multicall 0x{}",
            hex::encode(send_message_multicall.0)
        );
    }

    let provider = apis
        .first()
        .expect("workers must be greater than 0")
        .provider()
        .clone();

    // proportionally increase the channel size to workers and batch size
    // so that we can keep up with the load.
    // Also, code validation is quite slow and can create backpressure, so we want to be able to queue up a large number of batches if that happens.
    let (tx, rx) = tokio::sync::broadcast::channel(params.batch_size * params.workers * 512);

    let batch_pool = BatchPool::<SmallRng>::new(
        apis,
        params.ethexe_nodes.clone(),
        params.workers,
        params.batch_size,
        send_message_multicall,
        rx.resubscribe(),
    )?;

    let run_result = tokio::select! {
        r = utils::listen_blocks(tx, provider.root().clone()) => r,
        r = batch_pool.run(params, rx) => r,
    };

    run_result
}
