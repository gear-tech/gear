//! Batch execution engine for load mode.
//!
//! This module owns the long-running worker pool used by `ethexe-node-loader
//! load`. It is responsible for:
//!
//! - generating batches from the current execution context,
//! - executing them through Ethereum and ethexe RPC clients,
//! - observing block-by-block outcomes,
//! - folding the observed state back into the shared context.

use alloy::{
    consensus::Transaction,
    eips::BlockId,
    network::{BlockResponse, primitives::HeaderResponse},
    primitives::{Address, FixedBytes, U256},
    providers::{Provider, WalletProvider},
    rpc::types::{BlockTransactions, Filter, Header},
    sol_types::{SolCall, SolEvent},
};
use anyhow::Result;
use ethexe_common::{Address as EthexeAddress, events::MirrorEvent, injected::Promise};
use ethexe_ethereum::{
    Ethereum, TryGetReceipt,
    abi::{IMirror, IRouter::commitBatchCall},
    mirror::events::try_extract_event,
};
use futures::{FutureExt, StreamExt, stream::FuturesUnordered};
use gear_call_gen::{CallGenRng, ClaimValueArgs, SendReplyArgs};
use gear_core::{
    ids::prelude::{CodeIdExt, MessageIdExt},
    message::ReplyCode,
};
use gprimitives::{ActorId, CodeId, H256, MessageId};
use rand::{RngCore, SeedableRng, rngs::SmallRng};
use std::{
    collections::{BTreeMap, BTreeSet},
    marker::PhantomData,
    sync::Arc,
};
use tokio::sync::{
    RwLock,
    broadcast::{Receiver, error::RecvError},
    watch,
};
use tracing::instrument;

use crate::{
    abi::BatchMulticall,
    args::SeedVariant,
    batch::{
        context::{Context, ContextUpdate},
        generator::{Batch, BatchGenerator, BatchWithSeed, RuntimeSettings},
        report::{
            BatchExecutionStats, BatchReport, BatchRunReport, LoadRunMetadata, LoadRunReport,
            RunEndedBy,
        },
    },
    utils,
};

pub mod context;
pub mod generator;
pub mod report;
pub mod rpc_pool;

use rpc_pool::EthexeRpcPool;

pub struct BatchPool<Rng: CallGenRng> {
    apis: Vec<Ethereum>,
    rpc_pools: Vec<Option<EthexeRpcPool>>,
    pool_size: usize,
    batch_size: usize,
    send_message_multicall: Address,
    use_send_message_multicall: bool,
    context: Context,
    batch_stats: BatchExecutionStats,
    rx: Receiver<Header>,
    _marker: PhantomData<Rng>,
}

#[derive(Debug, Clone)]
pub struct LoadRunConfig {
    pub loader_seed: Option<u64>,
    pub code_seed_type: Option<SeedVariant>,
    pub workers: usize,
    pub batch_size: usize,
}

type MidMap = Arc<RwLock<BTreeMap<MessageId, ActorId>>>;
type WorkerBatchFuture =
    futures::future::BoxFuture<'static, (usize, EthexeRpcPool, Result<BatchRunReport>)>;

/// Amount of wVARA (12 decimals) to top up each program's executable balance.
const TOP_UP_AMOUNT: u128 = 500_000_000_000_000;

const INJECTED_TX_RATIO_NUM: u8 = 7;
const INJECTED_TX_RATIO_DEN: u8 = 10;
const MAX_MULTICALL_CALLDATA_BYTES: usize = 120 * 1024;

/// Biases message traffic toward injected transactions while keeping some
/// regular on-chain sends in circulation.
fn prefer_injected_tx(rng: &mut impl RngCore) -> bool {
    // Make injected txs common, but still keep some on-chain `send_message` calls.
    (rng.next_u32() % INJECTED_TX_RATIO_DEN as u32) < INJECTED_TX_RATIO_NUM as u32
}

/// Produces a fuzzed message value used for init, send, and reply operations.
fn fuzz_message_value(rng: &mut impl RngCore) -> u128 {
    // 60% zero value
    if rng.next_u32() % 10 < 6 {
        return 0;
    }

    // 40% random value
    let max_value = 1_000_000_000_000_000_000u128;
    let random_value = ((rng.next_u64() as u128) << 64) | (rng.next_u64() as u128);
    random_value % max_value
}

/// Converts an arbitrary salt buffer into the fixed 32-byte form expected by
/// Ethereum ABI bindings.
pub(crate) fn salt_to_h256(salt: &[u8]) -> H256 {
    let mut out = [0u8; 32];
    let take = salt.len().min(out.len());
    out[..take].copy_from_slice(&salt[..take]);
    H256::from_slice(&out)
}

/// Events emitted by mirror contract. Used to build mailbox and other context state for
/// batch report.
#[derive(Debug, Clone)]
pub struct Event {
    pub event: MirrorEvent,
    /// Actor id of the program whose mirror emitted the event.
    pub actor_id: ActorId,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct TransitionedMessage {
    id: MessageId,
    destination_is_mailbox: bool,
    replied_to: Option<MessageId>,
}

fn apply_mirror_event_update(context_update: &mut ContextUpdate, event: &Event) {
    let actor_id = event.actor_id;
    match &event.event {
        MirrorEvent::OwnedBalanceTopUpRequested(_) => {
            context_update.stats_mut(actor_id).increment_owned_topups();
        }
        MirrorEvent::ExecutableBalanceTopUpRequested(_) => {
            context_update
                .stats_mut(actor_id)
                .increment_executable_topups();
        }
        MirrorEvent::Message(ev) => {
            context_update.upsert_message_owner(ev.id, actor_id);
            context_update.stats_mut(actor_id).increment_messages();
        }
        MirrorEvent::MessageCallFailed(ev) => {
            context_update.upsert_message_owner(ev.id, actor_id);
            let stats = context_update.stats_mut(actor_id);
            stats.increment_messages();
            stats.increment_failures();
        }
        MirrorEvent::MessageQueueingRequested(ev) => {
            context_update.upsert_message_owner(ev.id, actor_id);
        }
        MirrorEvent::Reply(ev) => {
            context_update.upsert_message_owner(ev.reply_to, actor_id);
            context_update.upsert_message_owner(MessageId::generate_reply(ev.reply_to), actor_id);
            context_update.stats_mut(actor_id).increment_replies();
        }
        MirrorEvent::ReplyCallFailed(ev) => {
            context_update.upsert_message_owner(ev.reply_to, actor_id);
            context_update.upsert_message_owner(MessageId::generate_reply(ev.reply_to), actor_id);
            let stats = context_update.stats_mut(actor_id);
            stats.increment_replies();
            stats.increment_failures();
        }
        MirrorEvent::ReplyQueueingRequested(ev) => {
            context_update.upsert_message_owner(ev.replied_to, actor_id);
            context_update.upsert_message_owner(MessageId::generate_reply(ev.replied_to), actor_id);
            context_update.stats_mut(actor_id).increment_replies();
        }
        MirrorEvent::StateChanged(ev) => {
            context_update.set_program_last_state_hash(actor_id, ev.state_hash);
            context_update.stats_mut(actor_id).increment_state_changes();
        }
        MirrorEvent::ValueClaimed(ev) => {
            context_update.upsert_message_owner(ev.claimed_id, actor_id);
            context_update.remove_mailbox_message(actor_id, ev.claimed_id);
            context_update.remove_pending_value_claim(actor_id, ev.claimed_id);
            context_update
                .stats_mut(actor_id)
                .increment_claims_succeeded();
        }
        MirrorEvent::ValueClaimingRequested(ev) => {
            context_update.upsert_message_owner(ev.claimed_id, actor_id);
            context_update.add_pending_value_claim(actor_id, ev.claimed_id);
            context_update
                .stats_mut(actor_id)
                .increment_claims_requested();
        }
        MirrorEvent::TransferLockedValueToInheritorFailed(_)
        | MirrorEvent::ReplyTransferFailed(_) => {
            context_update.stats_mut(actor_id).increment_failures();
        }
        MirrorEvent::ValueClaimFailed(ev) => {
            context_update.upsert_message_owner(ev.claimed_id, actor_id);
            context_update.remove_pending_value_claim(actor_id, ev.claimed_id);
            let stats = context_update.stats_mut(actor_id);
            stats.increment_claims_failed();
            stats.increment_failures();
        }
    }
}

fn apply_router_transition_update(
    context_update: &mut ContextUpdate,
    actor_id: ActorId,
    exited: bool,
    value_claims: impl IntoIterator<Item = MessageId>,
    messages: impl IntoIterator<Item = TransitionedMessage>,
) {
    if exited {
        context_update.set_program_exited(actor_id, true);
    }

    for message_id in value_claims {
        context_update.upsert_message_owner(message_id, actor_id);
    }

    for message in messages {
        context_update.upsert_message_owner(message.id, actor_id);
        if message.destination_is_mailbox {
            context_update.add_mailbox_message(actor_id, message.id);
            context_update
                .stats_mut(actor_id)
                .increment_mailbox_additions();
        }

        if let Some(replied_to) = message.replied_to {
            context_update.upsert_message_owner(replied_to, actor_id);
            context_update.upsert_message_owner(MessageId::generate_reply(replied_to), actor_id);
        }
    }
}

impl<Rng: CallGenRng> BatchPool<Rng> {
    /// Creates a batch pool with one dedicated ethexe RPC pool per worker.
    pub fn new(
        apis: Vec<Ethereum>,
        ethexe_rpc_urls: Vec<String>,
        pool_size: usize,
        batch_size: usize,
        send_message_multicall: Address,
        use_send_message_multicall: bool,
        rx: Receiver<Header>,
    ) -> Result<Self> {
        let rpc_pools = (0..pool_size)
            .map(|worker_idx| {
                let pool = EthexeRpcPool::new(ethexe_rpc_urls.clone())?;
                tracing::info!(
                    worker_idx,
                    endpoints = pool.endpoint_count(),
                    "Initialized dedicated ethexe RPC pool for worker"
                );
                Ok(Some(pool))
            })
            .collect::<Result<Vec<_>>>()?;

        Ok(Self {
            apis,
            rpc_pools,
            pool_size,
            batch_size,
            send_message_multicall,
            use_send_message_multicall,
            context: Context::new(),
            batch_stats: BatchExecutionStats::default(),
            rx,
            _marker: PhantomData,
        })
    }

    /// Starts the batch pool and returns a summary once in-flight work drains.
    pub async fn run(
        mut self,
        config: LoadRunConfig,
        shutdown: watch::Receiver<bool>,
    ) -> Result<LoadRunReport> {
        self.run_pool_loop(config, shutdown).await
    }

    /// Continuously schedules one batch per worker and replaces each completed
    /// batch with a newly generated one.
    pub async fn run_pool_loop(
        &mut self,
        config: LoadRunConfig,
        mut shutdown: watch::Receiver<bool>,
    ) -> Result<LoadRunReport> {
        let mut batches = FuturesUnordered::<WorkerBatchFuture>::new();
        let mid_map = MidMap::default();
        let seed = config.loader_seed.unwrap_or_else(gear_utils::now_millis);
        let mut rpc_rng = SmallRng::seed_from_u64(seed ^ 0xA17E_7E11);
        let mut shutting_down = *shutdown.borrow();
        tracing::info!(
            message = "Running task pool with params",
            seed,
            pool_size = self.pool_size,
            batch_size = self.batch_size
        );

        let rt_settings = RuntimeSettings::new()?;
        let mut batch_gen =
            BatchGenerator::<Rng>::new(seed, self.batch_size, config.code_seed_type, rt_settings);

        if !shutting_down {
            for worker_idx in 0..self.pool_size {
                self.schedule_batch(
                    &mut batches,
                    &mut batch_gen,
                    &mid_map,
                    &mut rpc_rng,
                    worker_idx,
                );
            }
        }

        while !batches.is_empty() {
            let (worker_idx, rpc_pool, report) = tokio::select! {
                result = batches.next() => result.expect("batches should not be empty while waiting"),
                changed = shutdown.changed(), if !shutting_down => {
                    match changed {
                        Ok(()) if *shutdown.borrow() => {
                            shutting_down = true;
                            tracing::info!("Shutdown requested; draining in-flight batches");
                        }
                        Ok(()) => {}
                        Err(_) => {
                            shutting_down = true;
                        }
                    }
                    continue;
                }
            };

            self.rpc_pools[worker_idx] = Some(rpc_pool);
            match report {
                Ok(report) => self.process_run_report(report),
                Err(err) => {
                    self.batch_stats.record_failed();
                    tracing::error!(
                        worker_idx,
                        error = %err,
                        "Batch failed"
                    );
                }
            }

            if !shutting_down {
                self.schedule_batch(
                    &mut batches,
                    &mut batch_gen,
                    &mid_map,
                    &mut rpc_rng,
                    worker_idx,
                );
            }
        }

        Ok(LoadRunReport {
            metadata: LoadRunMetadata {
                seed,
                workers: config.workers,
                batch_size: config.batch_size,
            },
            ended_by: if *shutdown.borrow() {
                RunEndedBy::Interrupted
            } else {
                RunEndedBy::Completed
            },
            context: std::mem::take(&mut self.context),
            batch_stats: std::mem::take(&mut self.batch_stats),
        })
    }

    fn process_run_report(&mut self, report: BatchRunReport) {
        let BatchRunReport { seed, batch } = report;
        tracing::debug!(seed, "Processed batch report");
        self.batch_stats.record_completed();
        self.context.update(batch.context_update);
    }

    fn schedule_batch(
        &mut self,
        batches: &mut FuturesUnordered<WorkerBatchFuture>,
        batch_gen: &mut BatchGenerator<Rng>,
        mid_map: &MidMap,
        rpc_rng: &mut SmallRng,
        worker_idx: usize,
    ) {
        let batch_with_seed = batch_gen.generate(self.context.clone());
        let api = self.apis[worker_idx].clone();
        let rpc_pool = self.rpc_pools[worker_idx]
            .take()
            .expect("rpc pool must be present for worker");
        let endpoint_idx = rpc_pool.random_endpoint_index(rpc_rng);
        batches.push(
            run_batch_for_worker(
                worker_idx,
                api,
                rpc_pool,
                endpoint_idx,
                batch_with_seed,
                self.send_message_multicall,
                self.use_send_message_multicall,
                self.rx.resubscribe(),
                mid_map.clone(),
            )
            .boxed(),
        );
    }
}

#[allow(clippy::too_many_arguments)]
async fn run_batch(
    api: Ethereum,
    mut rpc_pool: EthexeRpcPool,
    endpoint_idx: usize,
    batch: BatchWithSeed,
    send_message_multicall: Address,
    use_send_message_multicall: bool,
    rx: Receiver<Header>,
    mid_map: MidMap,
) -> (EthexeRpcPool, Result<BatchRunReport>) {
    let (seed, batch) = batch.into();
    let mut rng = SmallRng::seed_from_u64(seed);

    let result = match run_batch_impl(
        api,
        &mut rpc_pool,
        endpoint_idx,
        batch,
        send_message_multicall,
        use_send_message_multicall,
        rx,
        mid_map,
        &mut rng,
    )
    .await
    {
        Ok(report) => Ok(BatchRunReport::new(seed, report)),
        Err(err) => {
            tracing::warn!("Batch failed: {err:?}");
            Err(err)
        }
    };
    (rpc_pool, result)
}

/// Small wrapper that preserves the worker index alongside a batch result.
#[allow(clippy::too_many_arguments)]
async fn run_batch_for_worker(
    worker_idx: usize,
    api: Ethereum,
    rpc_pool: EthexeRpcPool,
    endpoint_idx: usize,
    batch: BatchWithSeed,
    send_message_multicall: Address,
    use_send_message_multicall: bool,
    rx: Receiver<Header>,
    mid_map: MidMap,
) -> (usize, EthexeRpcPool, Result<BatchRunReport>) {
    tracing::debug!(
        worker_idx,
        endpoint_idx,
        "Running batch on pooled ethexe RPC endpoint"
    );
    let (rpc_pool, result) = run_batch(
        api,
        rpc_pool,
        endpoint_idx,
        batch,
        send_message_multicall,
        use_send_message_multicall,
        rx,
        mid_map,
    )
    .await;
    (worker_idx, rpc_pool, result)
}

/// Executes one generated batch and converts chain observations into a
/// [`Report`] that can update shared generator state.
#[instrument(skip_all)]
#[allow(clippy::too_many_arguments)]
async fn run_batch_impl(
    api: Ethereum,
    rpc_pool: &mut EthexeRpcPool,
    endpoint_idx: usize,
    batch: Batch,
    send_message_multicall: Address,
    use_send_message_multicall: bool,
    rx: Receiver<Header>,
    mid_map: MidMap,
    rng: &mut SmallRng,
) -> Result<BatchReport> {
    match batch {
        Batch::UploadProgram(args) => {
            tracing::info!(programs = args.len(), "Uploading programs");
            let mut code_ids = Vec::with_capacity(args.len());

            for arg in args.iter() {
                let expected_code_id = CodeId::generate(&arg.0.0);
                tracing::trace!(
                    code_id = %expected_code_id,
                    bytes = arg.0.0.len(),
                    "Requesting code validation"
                );
                let code_id = rpc_pool
                    .request_code_validation(endpoint_idx, &api, &arg.0.0)
                    .await?;
                assert_eq!(code_id, CodeId::generate(&arg.0.0));
                rpc_pool
                    .wait_for_code_validation(endpoint_idx, &api, code_id)
                    .await?;
                tracing::trace!(code_id = %code_id, "Code validated");
                code_ids.push(code_id);
            }

            let mut messages = BTreeMap::new();
            let block_number = api.provider().get_block_number().await?;

            let mut upload_calls = Vec::with_capacity(args.len());
            for (call_id, (arg, code_id)) in args.iter().zip(code_ids.iter().copied()).enumerate() {
                let salt = &arg.0.1;
                let fuzzed_value = fuzz_message_value(rng);
                upload_calls.push((
                    call_id,
                    code_id,
                    salt_to_h256(salt),
                    arg.0.2.clone(),
                    fuzzed_value,
                    TOP_UP_AMOUNT,
                ));
            }

            let (created, tx_count) =
                create_program_batch_via_multicall(&api, send_message_multicall, &upload_calls)
                    .await?;

            let mut created_programs = Vec::with_capacity(upload_calls.len());
            for (call_id, program_id, message_id) in created {
                mid_map.write().await.insert(message_id, program_id);
                messages.insert(message_id, (program_id, call_id));
                created_programs.push((call_id, program_id, message_id));
                tracing::trace!(
                    call_id,
                    %program_id,
                    %message_id,
                    "Program created"
                );
            }

            let wait_for_event_blocks = blocks_window(tx_count, 2, 6);
            let mut report = process_events(
                api,
                messages,
                rx,
                block_number,
                mid_map,
                wait_for_event_blocks,
                BTreeMap::new(),
            )
            .await?;

            for (_, code_id, ..) in &upload_calls {
                report.context_update.add_code(*code_id);
            }
            for (call_id, program_id, message_id) in created_programs {
                let code_id = upload_calls[call_id].1;
                report
                    .context_update
                    .set_program_code_id(program_id, code_id);
                report
                    .context_update
                    .upsert_message_owner(message_id, program_id);
            }

            Ok(report)
        }

        Batch::UploadCode(args) => {
            tracing::info!(codes = args.len(), "Uploading codes");
            let mut code_ids = Vec::with_capacity(args.len());
            let start = std::time::Instant::now();

            for arg in args.iter() {
                let expected_code_id = CodeId::generate(&arg.0);
                tracing::debug!(
                    code_id = %expected_code_id,
                    bytes = arg.0.len(),
                    "Requesting code validation"
                );
                let code_id = rpc_pool
                    .request_code_validation(endpoint_idx, &api, &arg.0)
                    .await?;
                assert_eq!(code_id, CodeId::generate(&arg.0));
                rpc_pool
                    .wait_for_code_validation(endpoint_idx, &api, code_id)
                    .await?;
                tracing::debug!(code_id = %code_id, "Code validated");
                code_ids.push(code_id);
            }

            tracing::debug!(
                codes = code_ids.len(),
                elapsed_ms = start.elapsed().as_millis(),
                "Codes validated"
            );

            let mut context_update = ContextUpdate::default();
            for code_id in code_ids {
                context_update.add_code(code_id);
            }

            Ok(BatchReport { context_update })
        }

        Batch::SendMessage(args) => {
            tracing::info!(messages = args.len(), "Sending messages");
            let mut messages = BTreeMap::new();
            let mut injected_promises: BTreeMap<MessageId, Promise> = BTreeMap::new();
            let block_number = api.provider().get_block_number().await?;
            let mut regular_calls = Vec::new();
            let mut injected_tx_count = 0usize;
            let mut regular_tx_count = 0usize;

            for (i, arg) in args.iter().enumerate() {
                let to = arg.0.0;
                let fuzzed_value = fuzz_message_value(rng);
                if prefer_injected_tx(rng) {
                    let (message_id, promise) = rpc_pool
                        .send_message_injected_and_watch(endpoint_idx, &api, to, &arg.0.1, 0)
                        .await?;
                    messages.insert(message_id, (to, i));
                    injected_promises.insert(message_id, promise);
                    mid_map.write().await.insert(message_id, to);
                    injected_tx_count = injected_tx_count.saturating_add(1);
                    tracing::trace!(call_id = i, %to, %message_id, "Injected message sent");
                } else {
                    regular_calls.push((i, to, arg.0.1.clone(), fuzzed_value));
                }
            }

            if !regular_calls.is_empty() {
                let (sent, tx_count) = if use_send_message_multicall {
                    send_message_batch_via_multicall(&api, send_message_multicall, &regular_calls)
                        .await?
                } else {
                    send_message_batch_direct(&api, &regular_calls).await?
                };
                regular_tx_count = tx_count;

                for (call_id, to, message_id) in sent {
                    messages.insert(message_id, (to, call_id));
                    mid_map.write().await.insert(message_id, to);
                    tracing::trace!(call_id, %to, %message_id, "Message sent");
                }
            }

            let dispatched_txs = injected_tx_count.saturating_add(regular_tx_count);
            let wait_for_event_blocks =
                send_message_wait_window(dispatched_txs, use_send_message_multicall);
            process_events(
                api,
                messages,
                rx,
                block_number,
                mid_map,
                wait_for_event_blocks,
                injected_promises,
            )
            .await
        }

        Batch::ClaimValue(args) => {
            let removed_from_mailbox = args.clone().into_iter().map(|ClaimValueArgs(mid)| mid);

            for (call_id, arg) in args.iter().enumerate() {
                let mid = arg.0;
                let actor_id = *mid_map
                    .read()
                    .await
                    .get(&mid)
                    .ok_or_else(|| anyhow::anyhow!("Actor not found for message id {mid}"))?;
                let mirror = api.mirror(actor_id);
                mirror.claim_value(mid).await?;
                tracing::trace!(call_id, %mid, "Value claimed");
            }

            let mut context_update = ContextUpdate::default();
            for mid in removed_from_mailbox {
                if let Some(actor_id) = mid_map.read().await.get(&mid).copied() {
                    context_update.remove_mailbox_message(actor_id, mid);
                    context_update.remove_pending_value_claim(actor_id, mid);
                    context_update
                        .stats_mut(actor_id)
                        .increment_claims_succeeded();
                }
            }

            Ok(BatchReport { context_update })
        }

        Batch::SendReply(args) => {
            let removed_from_mailbox = args.clone().into_iter().map(|SendReplyArgs((mid, ..))| mid);

            let mut messages = BTreeMap::new();
            let block_number = api.provider().get_block_number().await?;

            for (call_id, arg) in args.iter().enumerate() {
                let mid = arg.0.0;
                let payload = &arg.0.1;
                let fuzzed_value = fuzz_message_value(rng);
                let actor_id = *mid_map
                    .read()
                    .await
                    .get(&mid)
                    .ok_or_else(|| anyhow::anyhow!("Actor not found for message id {mid}"))?;
                let mirror = api.mirror(actor_id);
                let _ = mirror.send_reply(mid, payload, fuzzed_value).await?;
                let reply_mid = MessageId::generate_reply(mid);
                mid_map.write().await.insert(reply_mid, actor_id);
                messages.insert(mid, (actor_id, call_id));
                tracing::trace!(call_id, %mid, value = fuzzed_value, "Reply sent");
            }

            let blocks_per_action = 1;
            let wait_for_event_blocks = blocks_window(args.len(), blocks_per_action, 6);
            let event_mid_map = mid_map.clone();
            let mut report = process_events(
                api,
                messages,
                rx,
                block_number,
                event_mid_map,
                wait_for_event_blocks,
                BTreeMap::new(),
            )
            .await?;

            for mid in removed_from_mailbox {
                if let Some(actor_id) = mid_map.read().await.get(&mid).copied() {
                    report.context_update.remove_mailbox_message(actor_id, mid);
                }
            }

            Ok(report)
        }

        Batch::CreateProgram(args) => {
            tracing::info!(programs = args.len(), "Creating programs");
            let mut messages = BTreeMap::new();
            let block_number = api.provider().get_block_number().await?;

            let mut upload_calls = Vec::with_capacity(args.len());
            for (call_id, arg) in args.iter().enumerate() {
                let code_id = arg.0.0;
                let salt = &arg.0.1;
                let fuzzed_value = fuzz_message_value(rng);
                upload_calls.push((
                    call_id,
                    code_id,
                    salt_to_h256(salt),
                    arg.0.2.clone(),
                    fuzzed_value,
                    TOP_UP_AMOUNT,
                ));
            }

            let (created, tx_count) =
                create_program_batch_via_multicall(&api, send_message_multicall, &upload_calls)
                    .await?;

            let mut created_programs = Vec::with_capacity(upload_calls.len());
            for (call_id, program_id, message_id) in created {
                mid_map.write().await.insert(message_id, program_id);
                messages.insert(message_id, (program_id, call_id));
                created_programs.push((program_id, message_id));
                tracing::trace!(call_id, %program_id, %message_id, "Program created");
            }

            let wait_for_event_blocks = blocks_window(tx_count, 1, 6);
            let mut report = process_events(
                api,
                messages,
                rx,
                block_number,
                mid_map,
                wait_for_event_blocks,
                BTreeMap::new(),
            )
            .await?;

            for ((_, code_id, ..), (program_id, message_id)) in
                upload_calls.iter().zip(created_programs.iter())
            {
                report
                    .context_update
                    .set_program_code_id(*program_id, *code_id);
                report
                    .context_update
                    .upsert_message_owner(*message_id, *program_id);
            }

            Ok(report)
        }
    }
}

/// Sends a batch of `send_message` calls through the multicall helper.
///
/// Calls are automatically chunked so the encoded calldata stays under
/// [`MAX_MULTICALL_CALLDATA_BYTES`]. The returned `usize` is the number of
/// Ethereum transactions used to submit the whole batch.
async fn send_message_batch_via_multicall(
    api: &Ethereum,
    multicall_address: Address,
    calls: &[(usize, ActorId, Vec<u8>, u128)],
) -> Result<(Vec<(usize, ActorId, MessageId)>, usize)> {
    let multicall = BatchMulticall::new(multicall_address, api.provider());

    if calls.is_empty() {
        return Ok((Vec::new(), 0));
    }

    let mut mapped = Vec::with_capacity(calls.len());
    let mut tx_count = 0usize;
    let mut offset = 0usize;

    while offset < calls.len() {
        let chunk_start = offset;
        let mut value_sum = 0_u128;
        let mut chunk_end = offset;
        let mut batched_calls = Vec::new();

        while chunk_end < calls.len() {
            let (_, actor_id, payload, value) = &calls[chunk_end];
            let mut candidate_calls = batched_calls.clone();
            candidate_calls.push(BatchMulticall::MessageCall {
                mirror: Address::from(actor_id.to_address_lossy().0),
                payload: payload.clone().into(),
                value: *value,
            });

            let candidate_value_sum = value_sum.saturating_add(*value);
            let candidate_calldata_len = multicall
                .sendMessageBatch(candidate_calls.clone())
                .value(U256::from(candidate_value_sum));
            let candidate_calldata_len = candidate_calldata_len.calldata().len();

            tracing::trace!(
                chunk_start,
                calls = batched_calls.len(),
                calldata = candidate_calldata_len,
                "Multicall chunk candidate"
            );

            if candidate_calldata_len > MAX_MULTICALL_CALLDATA_BYTES {
                if batched_calls.is_empty() {
                    return Err(anyhow::anyhow!(
                        "single send_message call exceeds calldata limit: {} > {} bytes",
                        candidate_calldata_len,
                        MAX_MULTICALL_CALLDATA_BYTES
                    ));
                }

                tracing::debug!(
                    chunk_start,
                    split_at = chunk_end,
                    accepted = batched_calls.len(),
                    "Splitting multicall chunk"
                );

                break;
            }

            batched_calls = candidate_calls;
            value_sum = candidate_value_sum;
            chunk_end += 1;
        }

        tracing::debug!(
            chunk_start,
            chunk_end,
            calls = batched_calls.len(),
            value = value_sum,
            "Submitting multicall chunk"
        );

        let receipt = multicall
            .sendMessageBatch(batched_calls)
            .value(U256::from(value_sum))
            .send()
            .await?
            .try_get_receipt_check_reverted()
            .await?;

        let mut batch_result = None;
        for log in receipt.inner.logs() {
            if log.topic0() == Some(&BatchMulticall::SendMessageBatchResult::SIGNATURE_HASH) {
                let event = BatchMulticall::SendMessageBatchResult::decode_raw_log(
                    log.topics(),
                    &log.data().data,
                )?;
                batch_result = Some(event);
            }
        }

        let batch_result = batch_result
            .ok_or_else(|| anyhow::anyhow!("multicall send_message result event not found"))?;

        let chunk_calls = &calls[offset..chunk_end];
        if batch_result.messageIds.len() != chunk_calls.len() {
            return Err(anyhow::anyhow!(
                "multicall send_message result size mismatch: expected {}, got messageIds={}",
                chunk_calls.len(),
                batch_result.messageIds.len()
            ));
        }

        tracing::debug!(
            chunk_start,
            chunk_end,
            calls = chunk_calls.len(),
            returned = batch_result.messageIds.len(),
            "Multicall chunk result"
        );

        mapped.extend(
            chunk_calls.iter().zip(batch_result.messageIds).map(
                |((call_id, to, ..), message_id)| (*call_id, *to, MessageId::new(message_id.0)),
            ),
        );

        tx_count = tx_count.saturating_add(1);
        offset = chunk_end;
    }

    tracing::info!(
        calls = calls.len(),
        txs = tx_count,
        "send_message multicall complete"
    );

    Ok((mapped, tx_count))
}

async fn send_message_batch_direct(
    api: &Ethereum,
    calls: &[(usize, ActorId, Vec<u8>, u128)],
) -> Result<(Vec<(usize, ActorId, MessageId)>, usize)> {
    let mut mapped = Vec::with_capacity(calls.len());

    for (call_id, to, payload, value) in calls {
        let (_, message_id) = api.mirror(*to).send_message(payload, *value).await?;
        mapped.push((*call_id, *to, message_id));
    }

    tracing::info!(
        calls = calls.len(),
        txs = calls.len(),
        "send_message direct send complete"
    );

    Ok((mapped, calls.len()))
}

type Call = (usize, CodeId, H256, Vec<u8>, u128, u128);

/// Batch-create programs, send init messages, and top up executable balances via the multicall contract.
/// Each call contains (call_id, code_id, salt, init_payload, init_value, top_up_value).
/// Returns (call_id, program_id, message_id) for each created program.
async fn create_program_batch_via_multicall(
    api: &Ethereum,
    multicall_address: Address,
    calls: &[Call],
) -> Result<(Vec<(usize, ActorId, MessageId)>, usize)> {
    let multicall = BatchMulticall::new(multicall_address, api.provider());
    let router_address = Address(FixedBytes(api.router().address().0));

    if calls.is_empty() {
        return Ok((Vec::new(), 0));
    }

    let mut mapped = Vec::with_capacity(calls.len());
    let mut tx_count = 0usize;
    let mut offset = 0usize;

    while offset < calls.len() {
        let chunk_start = offset;
        let mut value_sum = 0_u128;
        let mut chunk_end = offset;
        let mut batched_calls = Vec::new();

        while chunk_end < calls.len() {
            let (_, code_id, salt, payload, value, top_up) = &calls[chunk_end];
            let mut candidate_calls = batched_calls.clone();
            candidate_calls.push(BatchMulticall::CreateProgramCall {
                codeId: code_id.into_bytes().into(),
                salt: salt.to_fixed_bytes().into(),
                initPayload: payload.clone().into(),
                initValue: *value,
                topUpValue: *top_up,
            });

            let candidate_value_sum = value_sum.saturating_add(*value);
            let candidate_calldata_len = multicall
                .createProgramBatch(router_address, candidate_calls.clone())
                .value(U256::from(candidate_value_sum));
            let candidate_calldata_len = candidate_calldata_len.calldata().len();

            tracing::trace!(
                chunk_start,
                calls = candidate_calls.len(),
                calldata = candidate_calldata_len,
                "Multicall chunk candidate"
            );

            if candidate_calldata_len > MAX_MULTICALL_CALLDATA_BYTES {
                if batched_calls.is_empty() {
                    return Err(anyhow::anyhow!(
                        "single create_program call exceeds calldata limit: {} > {} bytes",
                        candidate_calldata_len,
                        MAX_MULTICALL_CALLDATA_BYTES
                    ));
                }

                tracing::debug!(
                    chunk_start,
                    split_at = chunk_end,
                    accepted = batched_calls.len(),
                    "Splitting multicall chunk"
                );

                break;
            }

            batched_calls = candidate_calls;
            value_sum = candidate_value_sum;
            chunk_end += 1;
        }

        tracing::debug!(
            chunk_start,
            chunk_end,
            calls = batched_calls.len(),
            value = value_sum,
            "Submitting multicall chunk"
        );

        let receipt = multicall
            .createProgramBatch(router_address, batched_calls)
            .value(U256::from(value_sum))
            .send()
            .await?
            .try_get_receipt_check_reverted()
            .await?;

        let mut program_ids = Vec::new();
        let mut message_ids = Vec::new();

        for log in receipt.inner.logs() {
            if log.topic0() == Some(&ethexe_ethereum::router::events::signatures::PROGRAM_CREATED) {
                let event = ethexe_ethereum::abi::IRouter::ProgramCreated::decode_raw_log(
                    log.topics(),
                    &log.data().data,
                )?;
                program_ids.push(ActorId::from(*event.actorId.into_word()));
            } else if log.topic0()
                == Some(&ethexe_ethereum::mirror::signatures::MESSAGE_QUEUEING_REQUESTED)
            {
                let event = IMirror::MessageQueueingRequested::decode_raw_log(
                    log.topics(),
                    &log.data().data,
                )?;
                message_ids.push((*event.id).into());
            }
        }

        let chunk_calls = &calls[offset..chunk_end];
        if program_ids.len() != chunk_calls.len() || message_ids.len() != chunk_calls.len() {
            tracing::warn!(
                chunk_start,
                chunk_end,
                expected = chunk_calls.len(),
                programs = program_ids.len(),
                messages = message_ids.len(),
                "Fewer events than expected"
            );
        }

        tracing::debug!(
            chunk_start,
            chunk_end,
            calls = chunk_calls.len(),
            programs = program_ids.len(),
            messages = message_ids.len(),
            "Multicall chunk result"
        );

        mapped.extend(
            chunk_calls
                .iter()
                .zip(program_ids.into_iter().zip(message_ids))
                .map(|((call_id, ..), (pid, mid))| (*call_id, pid, mid)),
        );

        tx_count = tx_count.saturating_add(1);
        offset = chunk_end;
    }

    tracing::info!(
        calls = calls.len(),
        txs = tx_count,
        "create_program multicall complete"
    );

    Ok((mapped, tx_count))
}

/// Estimates how many future blocks should be scanned for outcomes of a batch.
fn blocks_window(action_count: usize, blocks_per_action: usize, headroom_blocks: usize) -> usize {
    action_count
        .saturating_mul(blocks_per_action)
        .saturating_add(headroom_blocks)
}

fn send_message_wait_window(action_count: usize, use_send_message_multicall: bool) -> usize {
    if use_send_message_multicall {
        blocks_window(action_count, 1, 6)
    } else {
        blocks_window(action_count, 2, 12)
    }
}

/// Parses `Router.commitBatch` transactions for the given block and extracts
/// mailbox, exit, and reply outcome information relevant to the tracked batch.
#[allow(clippy::too_many_arguments)]
async fn parse_router_transitions(
    api: &Ethereum,
    current_bn: FixedBytes<32>,
    to: Address,
    sent_message_ids: &BTreeSet<MessageId>,
    context_update: &mut ContextUpdate,
    mid_map: &MidMap,
    transition_outcomes: &mut BTreeMap<MessageId, Option<String>>,
) -> Result<()> {
    let full_block = api
        .provider()
        .get_block(BlockId::Hash(current_bn.into()))
        .full()
        .await?
        .expect("block not found?");

    if let BlockTransactions::Full(txs) = full_block.transactions() {
        for tx in txs {
            if let Some(tx_to) = tx.to()
                && tx_to.0.0 == api.router().address().0
            {
                match commitBatchCall::abi_decode(tx.input()) {
                    Ok(commit_batch) => {
                        let batch = commit_batch._batch;
                        tracing::trace!(
                            block = ?current_bn,
                            commitments = batch.chainCommitment.len(),
                            "Router.commitBatch"
                        );
                        for commitment in batch.chainCommitment.iter() {
                            for tr in commitment.transitions.iter() {
                                let actor_id: ActorId = EthexeAddress::from(tr.actorId).into();
                                if tr.exited {
                                    tracing::debug!(program = %actor_id, "Program exited");
                                }

                                let value_claim_ids: Vec<_> = tr
                                    .valueClaims
                                    .iter()
                                    .map(|vc| MessageId::new(vc.messageId.0))
                                    .collect();

                                let transitioned_messages: Vec<_> = tr
                                    .messages
                                    .iter()
                                    .map(|msg| {
                                        let msg_id = MessageId::new(msg.id.0);
                                        let replied_to = (msg.replyDetails.to.0 != [0u8; 32])
                                            .then(|| MessageId::new(msg.replyDetails.to.0));

                                        TransitionedMessage {
                                            id: msg_id,
                                            destination_is_mailbox: msg.destination == to,
                                            replied_to,
                                        }
                                    })
                                    .collect();

                                {
                                    let mut lock = mid_map.write().await;
                                    for message_id in value_claim_ids.iter().copied() {
                                        lock.insert(message_id, actor_id);
                                    }
                                    for message in &transitioned_messages {
                                        lock.insert(message.id, actor_id);
                                        if let Some(replied_to) = message.replied_to {
                                            lock.insert(replied_to, actor_id);
                                            lock.insert(
                                                MessageId::generate_reply(replied_to),
                                                actor_id,
                                            );
                                        }
                                    }
                                }

                                apply_router_transition_update(
                                    context_update,
                                    actor_id,
                                    tr.exited,
                                    value_claim_ids,
                                    transitioned_messages.iter().cloned(),
                                );

                                for msg in tr.messages.iter() {
                                    let is_reply = msg.replyDetails.to.0 != [0u8; 32];
                                    if is_reply {
                                        let replied_to = MessageId::new(msg.replyDetails.to.0);

                                        if sent_message_ids.contains(&replied_to) {
                                            let reply_code =
                                                ReplyCode::from_bytes(msg.replyDetails.code.0);
                                            let err = (!reply_code.is_success()).then(|| {
                                                String::from_utf8(msg.payload.to_vec())
                                                    .unwrap_or_else(|_| {
                                                        "<non-utf8 reply payload>".to_string()
                                                    })
                                            });

                                            let entry = transition_outcomes
                                                .entry(replied_to)
                                                .or_insert(Some("UNKNOWN".to_string()));
                                            match (&entry, &err) {
                                                (Some(current), None) if current == "UNKNOWN" => {
                                                    *entry = None;
                                                }
                                                (_, Some(_)) => {
                                                    *entry = err;
                                                }
                                                _ => {}
                                            }

                                            tracing::trace!(
                                                program = %actor_id,
                                                replied_to = %replied_to,
                                                success = reply_code.is_success(),
                                                "Reply outcome"
                                            );
                                        }
                                    }
                                }
                            }
                        }
                    }
                    Err(err) => {
                        tracing::trace!("Not a commit batch call: {}", err);
                    }
                }
            }
        }
    } else {
        tracing::trace!(
            block_hash = ?current_bn,
            "Block transactions are not available in Full form; skipping commitBatch parsing"
        );
    }

    Ok(())
}

/// Parses mirror contract logs for one block and updates the message-to-program
/// map used by reply and claim handling.
async fn parse_mirror_logs(
    api: &Ethereum,
    current_bn: FixedBytes<32>,
    mid_map: &MidMap,
    events: &mut Vec<Event>,
    context_update: &mut ContextUpdate,
) -> Result<()> {
    let logs = api
        .provider()
        .get_logs(&Filter::new().at_block_hash(current_bn))
        .await?;

    for log in logs {
        if let Some(mirror_event) = try_extract_event(&log)? {
            let actor_id: ActorId = EthexeAddress::from(log.address()).into();
            let event = Event {
                event: mirror_event,
                actor_id,
            };
            tracing::trace!(event = ?event.event, "Mirror event");

            {
                let mut lock = mid_map.write().await;
                match &event.event {
                    MirrorEvent::Reply(ev) => {
                        lock.insert(ev.reply_to, actor_id);
                        lock.insert(MessageId::generate_reply(ev.reply_to), actor_id);
                    }
                    MirrorEvent::ReplyCallFailed(ev) => {
                        lock.insert(ev.reply_to, actor_id);
                        lock.insert(MessageId::generate_reply(ev.reply_to), actor_id);
                    }
                    MirrorEvent::Message(ev) => {
                        lock.insert(ev.id, actor_id);
                    }
                    MirrorEvent::MessageCallFailed(ev) => {
                        lock.insert(ev.id, actor_id);
                    }
                    MirrorEvent::ValueClaimed(ev) => {
                        lock.insert(ev.claimed_id, actor_id);
                    }
                    _ => {}
                }
            }

            apply_mirror_event_update(context_update, &event);
            events.push(event);
        }
    }

    Ok(())
}

/// Receives the next header from the broadcast channel, transparently skipping
/// over lagged items.
async fn recv_next_header<T: Clone>(rx: &mut Receiver<T>) -> Result<T> {
    loop {
        match rx.recv().await {
            Ok(header) => return Ok(header),
            Err(RecvError::Lagged(skipped)) => {
                tracing::warn!(
                    skipped,
                    "Header subscription lagged; skipping stale headers and continuing"
                );
            }
            Err(err) => return Err(err.into()),
        }
    }
}

/// Wait for the new events since provided `block_number`.
/// For injected transactions with promises, uses the promise data directly.
/// For regular transactions, parses Router and Mirror events.
///
/// The resulting [`Report`] captures new codes, programs, mailbox mutations,
/// exits, and reply outcomes for the batch that was just submitted.
async fn process_events(
    api: Ethereum,
    mut messages: BTreeMap<MessageId, (ActorId, usize)>,
    mut rx: Receiver<Header>,
    block_number: u64,
    mid_map: MidMap,
    wait_for_event_blocks: usize,
    injected_promises: BTreeMap<MessageId, Promise>,
) -> Result<BatchReport> {
    let mut context_update = ContextUpdate::default();
    let initial_messages_len = messages.len();
    let injected_count = injected_promises.len();

    // Process injected transaction promises first - they already contain the reply info
    let mut results: BTreeMap<MessageId, Option<String>> = BTreeMap::new();
    for (mid, promise) in injected_promises {
        let reply_code = promise.reply.code;
        let status = if reply_code.is_success() {
            None
        } else {
            Some(String::from_utf8_lossy(&promise.reply.payload).to_string())
        };
        results.insert(mid, status);

        if let Some((actor_id, _)) = messages.get(&mid).copied() {
            context_update.upsert_message_owner(mid, actor_id);
        }
        messages.remove(&mid);
    }

    // For regular transactions, parse events from the chain
    if !messages.is_empty() {
        let mut block = recv_next_header(&mut rx).await?;
        while block.number < block_number {
            block = recv_next_header(&mut rx).await?;
        }

        tracing::info!(
            block = block.number,
            wait_blocks = wait_for_event_blocks,
            "Processing events"
        );

        let to: Address = api.provider().default_signer_address();
        let sent_message_ids: BTreeSet<MessageId> = messages.keys().copied().collect();
        let mut transition_outcomes: BTreeMap<MessageId, Option<String>> = BTreeMap::new();
        let mut v = Vec::new();
        let mut current_bn = block.hash();

        for _ in 0..wait_for_event_blocks {
            parse_router_transitions(
                &api,
                current_bn,
                to,
                &sent_message_ids,
                &mut context_update,
                &mid_map,
                &mut transition_outcomes,
            )
            .await?;

            parse_mirror_logs(&api, current_bn, &mid_map, &mut v, &mut context_update).await?;

            block = recv_next_header(&mut rx).await?;
            current_bn = block.hash();
        }

        for (mid, status) in
            utils::err_waited_or_succeed_batch(&mut v, messages.keys().copied()).await
        {
            results.insert(mid, status);
        }

        for (mid, status) in transition_outcomes {
            let entry = results.entry(mid).or_insert(Some("UNKNOWN".to_string()));
            match (&entry, &status) {
                (Some(current), None) if current == "UNKNOWN" => *entry = None,
                (None, Some(_)) => *entry = status,
                (Some(current), Some(_)) if current == "UNKNOWN" => *entry = status,
                _ => {}
            }
        }

        let resolved: BTreeSet<MessageId> = results.keys().copied().collect();
        for mid in messages.keys().copied() {
            if !resolved.contains(&mid) {
                results.insert(mid, Some("UNKNOWN".to_string()));
            }
        }
    }

    let mut ok_count = 0usize;
    let mut unknown_count = 0usize;
    let mut err_count = 0usize;
    for (_mid, status) in results.iter() {
        match status {
            None => ok_count += 1,
            Some(s) if s == "UNKNOWN" => unknown_count += 1,
            Some(_) => err_count += 1,
        }
    }

    for (mid, maybe_err) in &results {
        if let Some((pid, call_id)) = messages.remove(mid) {
            context_update.upsert_message_owner(*mid, pid);
            if let Some(expl) = maybe_err {
                tracing::debug!(call_id, %pid, %mid, error = %expl, "Call failed");
            } else {
                tracing::debug!(call_id, %pid, %mid, "Call succeeded");
            }
        }
    }

    for (mid, (pid, _)) in &messages {
        context_update.upsert_message_owner(*mid, *pid);
    }

    if !messages.is_empty() {
        tracing::error!(unresolved = ?messages, "Unresolved messages");
    }

    tracing::info!(
        total = initial_messages_len,
        injected = injected_count,
        ok = ok_count,
        err = err_count,
        unknown = unknown_count,
        "Batch results"
    );

    Ok(BatchReport { context_update })
}

#[cfg(test)]
mod tests {
    use super::{
        Event, TransitionedMessage, apply_mirror_event_update, apply_router_transition_update,
        send_message_wait_window,
    };
    use crate::batch::context::ContextUpdate;
    use ethexe_common::events::{
        MirrorEvent,
        mirror::{
            ExecutableBalanceTopUpRequestedEvent, OwnedBalanceTopUpRequestedEvent,
            ReplyCallFailedEvent, ReplyEvent, StateChangedEvent, ValueClaimFailedEvent,
            ValueClaimedEvent, ValueClaimingRequestedEvent,
        },
    };
    use gear_core::{ids::prelude::MessageIdExt, message::ReplyCode};
    use gprimitives::{ActorId, H256, MessageId};

    fn actor(seed: u8) -> ActorId {
        ActorId::from([seed; 32])
    }

    fn message(seed: u8) -> MessageId {
        MessageId::from([seed; 32])
    }

    fn hash(seed: u8) -> H256 {
        H256::from([seed; 32])
    }

    #[test]
    fn mirror_event_updates_mailbox_claims_and_state() {
        let actor_id = actor(1);
        let mid = message(2);

        let mut update = ContextUpdate::default();

        apply_mirror_event_update(
            &mut update,
            &Event {
                actor_id,
                event: MirrorEvent::ValueClaimingRequested(ValueClaimingRequestedEvent {
                    claimed_id: mid,
                    source: actor(9),
                }),
            },
        );
        apply_mirror_event_update(
            &mut update,
            &Event {
                actor_id,
                event: MirrorEvent::ValueClaimed(ValueClaimedEvent {
                    claimed_id: mid,
                    value: 0,
                }),
            },
        );
        apply_mirror_event_update(
            &mut update,
            &Event {
                actor_id,
                event: MirrorEvent::StateChanged(StateChangedEvent {
                    state_hash: hash(3),
                }),
            },
        );
        apply_mirror_event_update(
            &mut update,
            &Event {
                actor_id,
                event: MirrorEvent::ExecutableBalanceTopUpRequested(
                    ExecutableBalanceTopUpRequestedEvent { value: 10 },
                ),
            },
        );
        apply_mirror_event_update(
            &mut update,
            &Event {
                actor_id,
                event: MirrorEvent::OwnedBalanceTopUpRequested(OwnedBalanceTopUpRequestedEvent {
                    value: 11,
                }),
            },
        );
        apply_mirror_event_update(
            &mut update,
            &Event {
                actor_id,
                event: MirrorEvent::ValueClaimFailed(ValueClaimFailedEvent {
                    claimed_id: mid,
                    value: 0,
                }),
            },
        );

        let program = update.programs.get(&actor_id).expect("program update");
        assert!(program.pending_value_claims_removed.contains(&mid));
        assert_eq!(program.last_state_hash, Some(hash(3)));
        assert_eq!(program.stats_delta.claims_requested, 1);
        assert_eq!(program.stats_delta.claims_succeeded, 1);
        assert_eq!(program.stats_delta.claims_failed, 1);
        assert_eq!(program.stats_delta.state_changes, 1);
        assert_eq!(program.stats_delta.executable_topups, 1);
        assert_eq!(program.stats_delta.owned_topups, 1);
    }

    #[test]
    fn mirror_reply_events_register_message_owners_and_failures() {
        let actor_id = actor(4);
        let replied_to = message(5);

        let mut update = ContextUpdate::default();
        apply_mirror_event_update(
            &mut update,
            &Event {
                actor_id,
                event: MirrorEvent::Reply(ReplyEvent {
                    payload: b"ok".to_vec(),
                    value: 0,
                    reply_to: replied_to,
                    reply_code: ReplyCode::Unsupported,
                }),
            },
        );
        apply_mirror_event_update(
            &mut update,
            &Event {
                actor_id,
                event: MirrorEvent::ReplyCallFailed(ReplyCallFailedEvent {
                    value: 0,
                    reply_to: replied_to,
                    reply_code: ReplyCode::Unsupported,
                }),
            },
        );

        assert_eq!(update.message_owners.get(&replied_to), Some(&actor_id));
        assert_eq!(
            update
                .message_owners
                .get(&MessageId::generate_reply(replied_to)),
            Some(&actor_id)
        );
        let stats = &update.programs.get(&actor_id).expect("stats").stats_delta;
        assert_eq!(stats.replies, 2);
        assert_eq!(stats.failures, 1);
    }

    #[test]
    fn router_transition_marks_exit_and_mailbox_entries() {
        let actor_id = actor(6);
        let mailbox_mid = message(7);
        let claim_mid = message(8);
        let replied_to = message(9);

        let mut update = ContextUpdate::default();
        apply_router_transition_update(
            &mut update,
            actor_id,
            true,
            [claim_mid],
            [TransitionedMessage {
                id: mailbox_mid,
                destination_is_mailbox: true,
                replied_to: Some(replied_to),
            }],
        );

        let program = update.programs.get(&actor_id).expect("program update");
        assert_eq!(program.exited, Some(true));
        assert!(program.mailbox_added.contains(&mailbox_mid));
        assert_eq!(program.stats_delta.mailbox_additions, 1);
        assert_eq!(update.message_owners.get(&claim_mid), Some(&actor_id));
        assert_eq!(update.message_owners.get(&mailbox_mid), Some(&actor_id));
        assert_eq!(update.message_owners.get(&replied_to), Some(&actor_id));
    }

    #[test]
    fn direct_send_mode_waits_longer_for_events() {
        assert_eq!(send_message_wait_window(3, true), 9);
        assert_eq!(send_message_wait_window(3, false), 18);
    }
}
