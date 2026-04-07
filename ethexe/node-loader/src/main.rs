use crate::{
    abi::deploy_send_message_multicall,
    args::LoadParams,
    batch::{BatchPool, LoadRunConfig, report::RunEndedBy},
};
use alloy::{
    hex,
    primitives::Address,
    providers::{Provider, RootProvider},
    rpc::types::Header,
};
use anyhow::{Result, anyhow};
use args::{Params, parse_cli_params};
use ethexe_ethereum::{Ethereum, NO_BLOB_GAS_MULTIPLIER, NO_EIP1559_FEE_INCREASE_PERCENTAGE};
use rand::rngs::SmallRng;
use std::str::FromStr;
use tokio::{sync::broadcast, task::JoinSet};
use tracing::info;

mod abi;
mod args;
mod batch;
mod fuzz;
mod utils;

struct WorkerApis {
    apis: Vec<Ethereum>,
    addresses: Vec<gsigner::secp256k1::Address>,
}

/// Entrypoint for the node-loader CLI.
///
/// Runs a continuous load test against an `ethexe` dev node, generating randomized batches
/// that upload code/programs, send messages, send replies, and claim values to stress-test
/// the runtime and networking stack.
#[tokio::main]
async fn main() -> Result<()> {
    let fmt = tracing_subscriber::fmt::format().with_ansi(true).compact();
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

    let router_addr = validate_load_params(&params)?;
    let deployer_api = create_deployer_api(&params, router_addr).await?;
    let send_message_multicall = deploy_multicall(&deployer_api).await?;
    let WorkerApis { apis, addresses } = initialize_worker_apis(&params, router_addr).await?;

    fund_and_prepare_workers(
        &deployer_api,
        &apis,
        &addresses,
        send_message_multicall,
        MINT_AMOUNT,
    )
    .await?;

    let provider = apis
        .first()
        .expect("workers must be greater than 0")
        .provider()
        .clone();

    // proportionally increase the channel size to workers and batch size
    // so that we can keep up with the load.
    // Also, code validation is quite slow and can create backpressure, so we want to be able to queue up a large number of batches if that happens.
    let (tx, rx) = broadcast::channel(params.batch_size * params.workers * 512);

    let batch_pool = BatchPool::<SmallRng>::new(
        apis,
        params.ethexe_nodes.clone(),
        params.workers,
        params.batch_size,
        send_message_multicall,
        params.use_send_message_multicall,
        rx.resubscribe(),
    )?;

    run_load_runtime(
        batch_pool,
        LoadRunConfig {
            loader_seed: params.loader_seed,
            code_seed_type: params.code_seed_type,
            workers: params.workers,
            batch_size: params.batch_size,
        },
        tx,
        provider.root().clone(),
    )
    .await
}

fn validate_load_params(params: &LoadParams) -> Result<Address> {
    if params.workers == 0 {
        return Err(anyhow!("workers must be greater than 0"));
    }

    utils::validate_worker_count(params.ethexe_nodes.len(), params.workers)?;

    for arg in &params.ethexe_nodes {
        url::Url::parse(arg).map_err(|err| anyhow!("invalid Ethexe node URL '{arg}': {err}"))?;
    }

    Address::from_str(&params.router_address).map_err(Into::into)
}

async fn create_deployer_api(params: &LoadParams, router_addr: Address) -> Result<Ethereum> {
    let (deployer_signer, deployer_address) =
        if let Some(ref private_key) = params.sender_private_key {
            info!("Using provided sender private key");
            utils::signer_from_private_key(private_key)?
        } else {
            utils::signer_from_private_key(utils::DEPLOYER_ACCOUNT.private_key)?
        };

    info!("deployer address: 0x{}", hex::encode(deployer_address.0));
    info!(
        use_send_message_multicall = params.use_send_message_multicall,
        "Configured send_message execution mode"
    );

    Ethereum::new(
        &params.node,
        router_addr.into(),
        deployer_signer,
        deployer_address,
        NO_EIP1559_FEE_INCREASE_PERCENTAGE,
        NO_BLOB_GAS_MULTIPLIER,
    )
    .await
    .map_err(Into::into)
}

async fn deploy_multicall(deployer_api: &Ethereum) -> Result<Address> {
    let send_message_multicall = deploy_send_message_multicall(deployer_api).await?;
    info!(
        "send-message multicall deployed at 0x{}",
        hex::encode(send_message_multicall.0)
    );
    Ok(send_message_multicall)
}

async fn initialize_worker_apis(params: &LoadParams, router_addr: Address) -> Result<WorkerApis> {
    let mut init_tasks: JoinSet<Result<(u32, u32, gsigner::secp256k1::Address, Ethereum)>> =
        JoinSet::new();
    let worker_account_start = utils::worker_account_start(params.ethexe_nodes.len())?;
    for worker_idx in 0..params.workers as u32 {
        let account_index = worker_account_start + worker_idx;
        let (signer, address) = utils::signer_from_anvil_account(account_index)?;
        let node = params.node.clone();
        let router = router_addr;

        init_tasks.spawn(async move {
            let api = Ethereum::new(
                &node,
                router.into(),
                signer,
                address,
                NO_EIP1559_FEE_INCREASE_PERCENTAGE,
                NO_BLOB_GAS_MULTIPLIER,
            )
            .await?;
            Ok((worker_idx, account_index, address, api))
        });
    }

    let mut workers = Vec::with_capacity(params.workers);
    while let Some(result) = init_tasks.join_next().await {
        let (worker_idx, account_index, address, api) = result??;
        info!(
            "worker {worker_idx} (anvil account #{account_index}): 0x{}",
            hex::encode(address.0)
        );
        workers.push((worker_idx, address, api));
    }

    workers.sort_by_key(|(worker_idx, ..)| *worker_idx);
    let addresses = workers.iter().map(|(_, address, _)| *address).collect();
    let apis = workers.into_iter().map(|(_, _, api)| api).collect();

    Ok(WorkerApis { apis, addresses })
}

async fn fund_and_prepare_workers(
    deployer_api: &Ethereum,
    apis: &[Ethereum],
    worker_addresses: &[gsigner::secp256k1::Address],
    send_message_multicall: Address,
    mint_amount: u128,
) -> Result<()> {
    for (address, api) in worker_addresses.iter().zip(apis.iter()) {
        deployer_api
            .wrapped_vara()
            .mint((*address).into(), mint_amount)
            .await?;
        tracing::debug!(
            "Minted {} WVARA to 0x{}",
            mint_amount,
            hex::encode(address.0)
        );

        api.wrapped_vara().approve_all((*address).into()).await?;
        tracing::debug!("Approved all WVARA for 0x{}", hex::encode(address.0));

        api.wrapped_vara()
            .approve_all(send_message_multicall.into())
            .await?;
        tracing::debug!(
            "Approved all WVARA for multicall 0x{}",
            hex::encode(send_message_multicall.0)
        );
    }

    Ok(())
}

async fn run_load_runtime(
    batch_pool: BatchPool<SmallRng>,
    config: LoadRunConfig,
    tx: broadcast::Sender<Header>,
    provider: RootProvider,
) -> Result<()> {
    let (pool_shutdown_tx, pool_shutdown_rx) = tokio::sync::watch::channel(false);
    let (listener_shutdown_tx, listener_shutdown_rx) = tokio::sync::watch::channel(false);
    let pool_task = batch_pool.run(config, pool_shutdown_rx);
    let block_listener = utils::listen_blocks(tx, provider, listener_shutdown_rx);
    let ctrl_c = tokio::signal::ctrl_c();

    tokio::pin!(pool_task);
    tokio::pin!(block_listener);
    tokio::pin!(ctrl_c);

    let mut interrupted = false;
    let mut pool_result = None;
    let mut listener_result = None;

    while pool_result.is_none() || listener_result.is_none() {
        tokio::select! {
            result = &mut pool_task, if pool_result.is_none() => {
                pool_result = Some(result);
                let _ = listener_shutdown_tx.send(true);
            }
            result = &mut block_listener, if listener_result.is_none() => {
                listener_result = Some(result);
                let _ = pool_shutdown_tx.send(true);
            }
            signal = &mut ctrl_c, if !interrupted => {
                signal?;
                interrupted = true;
                info!("Ctrl+C received; stopping new batches and draining in-flight work");
                let _ = pool_shutdown_tx.send(true);
            }
        }
    }

    let mut run_report = pool_result.expect("pool task should finish")?;
    if interrupted {
        run_report.ended_by = RunEndedBy::Interrupted;
    }

    match listener_result.expect("block listener should finish") {
        Ok(()) => {
            println!("{run_report}");
            Ok(())
        }
        Err(err) => {
            run_report.ended_by = RunEndedBy::Failed;
            println!("{run_report}");
            Err(err)
        }
    }
}
