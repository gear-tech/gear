//! Load generator and fuzzing harness for local `ethexe` nodes.
//!
//! `ethexe-node-loader` is a binary crate used during local development to put
//! an `ethexe` deployment under sustained, mixed traffic. The loader operates in
//! three modes:
//!
//! - `load`: continuously creates randomized batches that upload code, create
//!   programs, send messages, send replies, and claim value;
//! - `fuzz`: deploys the demo syscall contract and repeatedly sends randomized
//!   command sequences to it;
//! - `dump`: materializes a generated Gear WASM module for a fixed seed to help
//!   with reproducing failures.
//!
//! In load mode, the crate either uses explicitly supplied worker private keys
//! or derives worker accounts from the standard Anvil mnemonic, funds them
//! through the configured deployer account, deploys a multicall helper contract,
//! and then keeps a pool of worker tasks running in parallel. A block
//! subscription drives event collection so the loader can keep track of created
//! programs, mailbox state, and reply outcomes between batches.

use crate::{
    abi::deploy_send_message_multicall,
    args::LoadParams,
    batch::{
        BatchPool, LoadRunConfig, WorkloadPolicy,
        report::RunEndedBy,
        value::{ValuePolicy, format_wvara},
    },
};
use alloy::{
    hex,
    primitives::Address,
    providers::{Provider, RootProvider},
    rpc::types::Header,
};
use anyhow::{Result, anyhow};
use args::{Params, parse_cli_params};
use ethexe_ethereum::{Ethereum, EthereumBuilder};
use rand::rngs::SmallRng;
use std::str::FromStr;
use tokio::{sync::broadcast, task::JoinSet};
use tracing::info;

mod abi;
mod args;
mod batch;
mod fuzz;
mod utils;

const DEFAULT_WORKER_MINT_AMOUNT: u128 = 500_000_000_000_000_000_000_000;
const MIN_POLICY_WORKER_MINT_AMOUNT: u128 = 1_000_000_000_000_000;

struct WorkerApis {
    apis: Vec<Ethereum>,
    addresses: Vec<gsigner::secp256k1::Address>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct WorkerFundingPlan {
    is_sender: bool,
    mint_amount: u128,
    approve_self: bool,
    approve_multicall: bool,
}

/// Parses CLI arguments, initializes tracing, and dispatches to the selected mode.
///
/// The command supports:
///
/// - [`Params::Dump`] for deterministic WASM generation from a seed,
/// - [`Params::Load`] for continuous mixed-workload generation,
/// - [`Params::Fuzz`] for syscall fuzzing against the demo mega contract.
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

            load_node(*load_params).await
        }
        Params::Fuzz(fuzz_params) => {
            info!("Starting syscall fuzz test on {}", fuzz_params.node);

            fuzz::run_fuzz(fuzz_params).await
        }
    }
}

/// Boots the load-testing workflow for a single Ethereum RPC endpoint.
///
/// The setup sequence is:
///
/// 1. validate worker account configuration and ethexe RPC URLs,
/// 2. create the deployer client and deploy the multicall helper,
/// 3. initialize one Ethereum client per worker account,
/// 4. fund and approve workers,
/// 5. start the block listener and the batch worker pool.
async fn load_node(params: LoadParams) -> Result<()> {
    let value_policy = ValuePolicy::from_parts(
        params.value_profile,
        params.max_msg_value,
        params.max_top_up_value,
        params.total_msg_value_budget,
        params.total_top_up_budget,
    )?;

    let value_policy_log = value_policy
        .as_ref()
        .map(ValuePolicy::describe)
        .unwrap_or_else(|| "disabled".to_string());
    info!(policy = %value_policy_log, "Configured value policy");

    let router_addr = validate_load_params(&params)?;
    let deployer_api = create_deployer_api(&params, router_addr).await?;
    let send_message_multicall = resolve_multicall(&params, &deployer_api).await?;
    let WorkerApis { apis, addresses } = initialize_worker_apis(&params, router_addr).await?;
    let worker_mint_amount = worker_mint_amount(params.mint_amount, value_policy.as_ref());
    info!(
        amount = %format_wvara(worker_mint_amount),
        "Configured worker WVARA target balance"
    );

    fund_and_prepare_workers(
        &deployer_api,
        &apis,
        &addresses,
        send_message_multicall,
        worker_mint_amount,
    )
    .await?;

    let provider = apis
        .first()
        .expect("workers must be greater than 0")
        .provider()
        .clone();

    let (tx, rx) = broadcast::channel(4096);

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
            workload_policy: WorkloadPolicy::new(params.program_creation_ratio),
            value_policy: value_policy.clone(),
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

    validate_worker_private_keys_count(params.workers, &params.worker_private_keys)?;
    if params.worker_private_keys.is_empty() {
        utils::validate_worker_count(params.ethexe_nodes.len(), params.workers)?;
    }

    for arg in &params.ethexe_nodes {
        url::Url::parse(arg).map_err(|err| anyhow!("invalid Ethexe node URL '{arg}': {err}"))?;
    }

    Address::from_str(&params.router_address).map_err(Into::into)
}

fn validate_worker_private_keys_count(
    workers: usize,
    worker_private_keys: &[String],
) -> Result<()> {
    if !worker_private_keys.is_empty() && worker_private_keys.len() != workers {
        return Err(anyhow!(
            "worker private key count ({}) must match workers ({workers})",
            worker_private_keys.len()
        ));
    }

    Ok(())
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

    EthereumBuilder::default()
        .rpc_url(params.node.clone())
        .router_address(router_addr.into())
        .signer(deployer_signer.clone())
        .sender_address(deployer_address)
        .blob_gas_multiplier(params.blob_gas_multiplier)
        .eip1559_fee_increase_percentage(params.eip1559_fee_increase_percentage)
        .build()
        .await
}

async fn resolve_multicall(params: &LoadParams, deployer_api: &Ethereum) -> Result<Address> {
    if let Some(address) = &params.send_message_multicall_address {
        let address = Address::from_str(address)?;
        info!(
            "reusing send-message multicall at 0x{}",
            hex::encode(address.0)
        );
        return Ok(address);
    }

    let send_message_multicall = deploy_send_message_multicall(deployer_api).await?;
    info!(
        "send-message multicall deployed at 0x{}",
        hex::encode(send_message_multicall.0)
    );
    Ok(send_message_multicall)
}

async fn initialize_worker_apis(params: &LoadParams, router_addr: Address) -> Result<WorkerApis> {
    enum WorkerAccountSource {
        Anvil(u32),
        PrivateKey,
    }

    let mut init_tasks: JoinSet<
        Result<(
            usize,
            WorkerAccountSource,
            gsigner::secp256k1::Address,
            Ethereum,
        )>,
    > = JoinSet::new();
    for worker_idx in 0..params.workers {
        let (source, signer, address) =
            if let Some(private_key) = params.worker_private_keys.get(worker_idx) {
                let (signer, address) = utils::signer_from_private_key(private_key)?;
                (WorkerAccountSource::PrivateKey, signer, address)
            } else {
                let worker_account_start = utils::worker_account_start(params.ethexe_nodes.len())?;
                let account_index = worker_account_start + u32::try_from(worker_idx)?;
                let (signer, address) = utils::signer_from_anvil_account(account_index)?;
                (WorkerAccountSource::Anvil(account_index), signer, address)
            };
        let node = params.node.clone();
        let router = router_addr;
        let blob_gas_multiplier = params.blob_gas_multiplier;
        let eip1559_fee_increase_percentage = params.eip1559_fee_increase_percentage;

        init_tasks.spawn(async move {
            let api = EthereumBuilder::default()
                .rpc_url(&node)
                .router_address(router.into())
                .signer(signer)
                .sender_address(address)
                .blob_gas_multiplier(blob_gas_multiplier)
                .eip1559_fee_increase_percentage(eip1559_fee_increase_percentage)
                .build()
                .await?;
            Ok((worker_idx, source, address, api))
        });
    }

    let mut workers = Vec::with_capacity(params.workers);
    while let Some(result) = init_tasks.join_next().await {
        let (worker_idx, source, address, api) = result??;
        match source {
            WorkerAccountSource::Anvil(account_index) => {
                info!(
                    "worker {worker_idx} (anvil account #{account_index}): 0x{}",
                    hex::encode(address.0)
                );
            }
            WorkerAccountSource::PrivateKey => {
                info!(
                    "worker {worker_idx} (provided account): 0x{}",
                    hex::encode(address.0)
                );
            }
        }
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
    target_balance: u128,
) -> Result<()> {
    let sender_address = deployer_api.sender_address();

    for (address, api) in worker_addresses.iter().zip(apis.iter()) {
        let balance = deployer_api
            .wrapped_vara()
            .query()
            .balance_of((*address).into())
            .await?;
        let plan = worker_funding_plan(*address, sender_address, target_balance, balance);

        if plan.is_sender {
            tracing::debug!(
                "Worker 0x{} uses the sender account",
                hex::encode(address.0)
            );
        }

        if plan.mint_amount > 0 {
            tracing::debug!(
                "Funding worker 0x{} with {} WVARA",
                hex::encode(address.0),
                plan.mint_amount
            );
            deployer_api
                .wrapped_vara()
                .mint((*address).into(), plan.mint_amount)
                .await?;
            tracing::debug!(
                "Minted {} WVARA to 0x{}",
                plan.mint_amount,
                hex::encode(address.0)
            );
        } else {
            tracing::debug!(
                "Worker 0x{} already has {} WVARA, target is {} WVARA",
                hex::encode(address.0),
                balance,
                target_balance
            );
        }

        if plan.approve_self {
            tracing::debug!("Approving all WVARA for 0x{}", hex::encode(address.0));
            api.wrapped_vara().approve_all((*address).into()).await?;
            tracing::debug!("Approved all WVARA for 0x{}", hex::encode(address.0));
        } else {
            tracing::debug!(
                "Skipping self WVARA approval for sender worker 0x{}",
                hex::encode(address.0)
            );
        }

        if plan.approve_multicall {
            tracing::debug!(
                "Approving all WVARA for multicall 0x{} from worker 0x{}",
                hex::encode(send_message_multicall.0),
                hex::encode(address.0)
            );
            api.wrapped_vara()
                .approve_all(send_message_multicall.into())
                .await?;
            tracing::debug!(
                "Approved all WVARA for multicall 0x{}",
                hex::encode(send_message_multicall.0)
            );
        }
    }

    Ok(())
}

fn worker_funding_plan(
    worker_address: gsigner::secp256k1::Address,
    sender_address: gsigner::secp256k1::Address,
    target_balance: u128,
    current_balance: u128,
) -> WorkerFundingPlan {
    let is_sender = worker_address == sender_address;

    WorkerFundingPlan {
        is_sender,
        mint_amount: target_balance.saturating_sub(current_balance),
        approve_self: !is_sender,
        approve_multicall: true,
    }
}

fn worker_mint_amount(override_amount: Option<u128>, policy: Option<&ValuePolicy>) -> u128 {
    if let Some(amount) = override_amount {
        return amount;
    }

    let Some(policy) = policy else {
        return DEFAULT_WORKER_MINT_AMOUNT;
    };

    let Some(total_budget) = policy.total_top_up_budget else {
        return DEFAULT_WORKER_MINT_AMOUNT;
    };

    total_budget
        .max(policy.max_top_up_value.unwrap_or_default())
        .max(MIN_POLICY_WORKER_MINT_AMOUNT)
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

#[cfg(test)]
mod tests {
    use super::{
        DEFAULT_WORKER_MINT_AMOUNT, MIN_POLICY_WORKER_MINT_AMOUNT, WorkerFundingPlan,
        validate_worker_private_keys_count, worker_funding_plan, worker_mint_amount,
    };
    use crate::batch::value::{ValuePolicy, ValueProfile};

    fn keys(count: usize) -> Vec<String> {
        (0..count).map(|idx| format!("0x{idx}")).collect()
    }

    #[test]
    fn worker_private_keys_can_be_omitted_for_anvil_fallback() {
        validate_worker_private_keys_count(3, &[]).expect("no manual keys");
    }

    #[test]
    fn worker_private_keys_must_match_worker_count_when_supplied() {
        validate_worker_private_keys_count(2, &keys(2)).expect("matching keys");

        let err = validate_worker_private_keys_count(2, &keys(1)).expect_err("mismatched keys");
        assert_eq!(
            err.to_string(),
            "worker private key count (1) must match workers (2)"
        );
    }

    #[test]
    fn worker_mint_amount_uses_explicit_override() {
        let policy = ValuePolicy::from_parts(Some(ValueProfile::Mainnet), None, None, None, None)
            .expect("policy");

        assert_eq!(worker_mint_amount(Some(42), policy.as_ref()), 42);
    }

    #[test]
    fn worker_mint_amount_uses_dev_fallback_when_uncapped() {
        assert_eq!(worker_mint_amount(None, None), DEFAULT_WORKER_MINT_AMOUNT);

        let policy = ValuePolicy::from_parts(Some(ValueProfile::Dev), None, None, None, None)
            .expect("policy");
        assert_eq!(
            worker_mint_amount(None, policy.as_ref()),
            DEFAULT_WORKER_MINT_AMOUNT
        );
    }

    #[test]
    fn worker_mint_amount_follows_top_up_budget_with_floor() {
        let policy = ValuePolicy::from_parts(None, None, Some(9), None, Some(17))
            .expect("policy")
            .expect("enabled");
        assert_eq!(
            worker_mint_amount(None, Some(&policy)),
            MIN_POLICY_WORKER_MINT_AMOUNT
        );

        let policy = ValuePolicy::from_parts(
            None,
            None,
            Some(MIN_POLICY_WORKER_MINT_AMOUNT + 5),
            None,
            Some(MIN_POLICY_WORKER_MINT_AMOUNT + 3),
        )
        .expect("policy")
        .expect("enabled");
        assert_eq!(
            worker_mint_amount(None, Some(&policy)),
            MIN_POLICY_WORKER_MINT_AMOUNT + 5
        );
    }

    #[test]
    fn sender_worker_funding_plan_skips_self_approval() {
        let worker = gsigner::secp256k1::Address([1; 20]);
        let sender = worker;

        assert_eq!(
            worker_funding_plan(worker, sender, 100, 40),
            WorkerFundingPlan {
                is_sender: true,
                mint_amount: 60,
                approve_self: false,
                approve_multicall: true,
            }
        );
    }

    #[test]
    fn non_sender_worker_funding_plan_keeps_worker_approvals() {
        let worker = gsigner::secp256k1::Address([1; 20]);
        let sender = gsigner::secp256k1::Address([2; 20]);

        assert_eq!(
            worker_funding_plan(worker, sender, 100, 140),
            WorkerFundingPlan {
                is_sender: false,
                mint_amount: 0,
                approve_self: true,
                approve_multicall: true,
            }
        );
    }
}
