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
use futures::{StreamExt, stream::FuturesUnordered};
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
};
use tracing::instrument;

use crate::{
    abi::BatchMulticall,
    args::{LoadParams, SeedVariant},
    batch::{
        context::Context,
        generator::{Batch, BatchGenerator, BatchWithSeed, RuntimeSettings},
        report::{BatchRunReport, MailboxReport, Report},
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
    task_context: Context,
    rx: Receiver<Header>,
    _marker: PhantomData<Rng>,
}

type MidMap = Arc<RwLock<BTreeMap<MessageId, ActorId>>>;

/// Amount of wVARA (12 decimals) to top up each program's executable balance.
const TOP_UP_AMOUNT: u128 = 500_000_000_000_000;

const INJECTED_TX_RATIO_NUM: u8 = 7;
const INJECTED_TX_RATIO_DEN: u8 = 10;
const MAX_MULTICALL_CALLDATA_BYTES: usize = 120 * 1024;

fn prefer_injected_tx(rng: &mut impl RngCore) -> bool {
    // Make injected txs common, but still keep some on-chain `send_message` calls.
    (rng.next_u32() % INJECTED_TX_RATIO_DEN as u32) < INJECTED_TX_RATIO_NUM as u32
}

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
    #[allow(dead_code)]
    pub actor_id: ActorId,
}

impl<Rng: CallGenRng> BatchPool<Rng> {
    pub fn new(
        apis: Vec<Ethereum>,
        ethexe_rpc_urls: Vec<String>,
        pool_size: usize,
        batch_size: usize,
        send_message_multicall: Address,
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
            task_context: Context::new(),
            rx,
            _marker: PhantomData,
        })
    }

    pub async fn run(mut self, params: LoadParams, _rx: Receiver<Header>) -> Result<()> {
        let run_pool_task = self.run_pool_loop(params.loader_seed, params.code_seed_type);

        let run_result = tokio::select! {
            r = run_pool_task => r,
        };

        run_result
    }

    pub async fn run_pool_loop(
        &mut self,
        loader_seed: Option<u64>,
        code_seed_type: Option<SeedVariant>,
    ) -> Result<()> {
        let mut batches = FuturesUnordered::new();
        let mid_map = MidMap::default();
        let seed = loader_seed.unwrap_or_else(gear_utils::now_millis);
        let mut rpc_rng = SmallRng::seed_from_u64(seed ^ 0xA17E_7E11);
        tracing::info!(
            message = "Running task pool with params",
            seed,
            pool_size = self.pool_size,
            batch_size = self.batch_size
        );

        let rt_settings = RuntimeSettings::new()?;
        let mut batch_gen =
            BatchGenerator::<Rng>::new(seed, self.batch_size, code_seed_type, rt_settings);

        for worker_idx in 0..self.pool_size {
            let batch_with_seed = batch_gen.generate(self.task_context.clone());
            let api = self.apis[worker_idx].clone();
            let rpc_pool = self.rpc_pools[worker_idx]
                .take()
                .expect("rpc pool must be present for worker");
            let endpoint_idx = rpc_pool.random_endpoint_index(&mut rpc_rng);
            batches.push(run_batch_for_worker(
                worker_idx,
                api,
                rpc_pool,
                endpoint_idx,
                batch_with_seed,
                self.send_message_multicall,
                self.rx.resubscribe(),
                mid_map.clone(),
            ));
        }

        while let Some((worker_idx, rpc_pool, report)) = batches.next().await {
            self.rpc_pools[worker_idx] = Some(rpc_pool);
            match report {
                Ok(report) => self.process_run_report(report),
                Err(err) => {
                    tracing::error!(
                        worker_idx,
                        error = %err,
                        "Batch failed, scheduling next batch for worker"
                    );
                }
            }

            let batch_with_seed = batch_gen.generate(self.task_context.clone());
            let api = self.apis[worker_idx].clone();
            let rpc_pool = self.rpc_pools[worker_idx]
                .take()
                .expect("rpc pool must be present for worker");
            let endpoint_idx = rpc_pool.random_endpoint_index(&mut rpc_rng);
            batches.push(run_batch_for_worker(
                worker_idx,
                api,
                rpc_pool,
                endpoint_idx,
                batch_with_seed,
                self.send_message_multicall,
                self.rx.resubscribe(),
                mid_map.clone(),
            ));
        }

        unreachable!()
    }

    fn process_run_report(&mut self, report: BatchRunReport) {
        self.task_context.update(report.context_update);
    }
}

async fn run_batch(
    api: Ethereum,
    mut rpc_pool: EthexeRpcPool,
    endpoint_idx: usize,
    batch: BatchWithSeed,
    send_message_multicall: Address,
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

#[allow(clippy::too_many_arguments)]
async fn run_batch_for_worker(
    worker_idx: usize,
    api: Ethereum,
    rpc_pool: EthexeRpcPool,
    endpoint_idx: usize,
    batch: BatchWithSeed,
    send_message_multicall: Address,
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
        rx,
        mid_map,
    )
    .await;
    (worker_idx, rpc_pool, result)
}

#[instrument(skip_all)]
#[allow(clippy::too_many_arguments)]
async fn run_batch_impl(
    api: Ethereum,
    rpc_pool: &mut EthexeRpcPool,
    endpoint_idx: usize,
    batch: Batch,
    send_message_multicall: Address,
    rx: Receiver<Header>,
    mid_map: MidMap,
    rng: &mut SmallRng,
) -> Result<Report> {
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

            let mut program_ids = BTreeSet::new();
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

            for (call_id, program_id, message_id) in created {
                program_ids.insert(program_id);
                mid_map.write().await.insert(message_id, program_id);
                messages.insert(message_id, (program_id, call_id));
                tracing::trace!(
                    call_id,
                    %program_id,
                    %message_id,
                    "Program created"
                );
            }

            let wait_for_event_blocks = blocks_window(tx_count, 2, 6);
            process_events(
                api,
                messages,
                rx,
                block_number,
                mid_map,
                wait_for_event_blocks,
                BTreeMap::new(),
            )
            .await
        }

        Batch::UploadCode(args) => {
            tracing::info!(codes = args.len(), "Uploading codes");
            let mut code_ids = Vec::with_capacity(args.len());
            let start = std::time::Instant::now();

            for arg in args.iter() {
                let expected_code_id = CodeId::generate(&arg.0);
                tracing::trace!(
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
                tracing::trace!(code_id = %code_id, "Code validated");
                code_ids.push(code_id);
            }

            tracing::debug!(
                codes = code_ids.len(),
                elapsed_ms = start.elapsed().as_millis(),
                "Codes validated"
            );

            Ok(Report {
                codes: code_ids.into_iter().collect(),
                ..Default::default()
            })
        }

        Batch::SendMessage(args) => {
            tracing::info!(messages = args.len(), "Sending messages");
            let mut messages = BTreeMap::new();
            let mut injected_promises: BTreeMap<MessageId, Promise> = BTreeMap::new();
            let block_number = api.provider().get_block_number().await?;
            let mut regular_calls = Vec::new();
            let mut injected_tx_count = 0usize;
            let mut multicall_tx_count = 0usize;

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
                let (sent, tx_count) =
                    send_message_batch_via_multicall(&api, send_message_multicall, &regular_calls)
                        .await?;
                multicall_tx_count = tx_count;

                for (call_id, to, message_id) in sent {
                    messages.insert(message_id, (to, call_id));
                    mid_map.write().await.insert(message_id, to);
                    tracing::trace!(call_id, %to, %message_id, "Message sent");
                }
            }

            let dispatched_txs = injected_tx_count.saturating_add(multicall_tx_count);
            let wait_for_event_blocks = blocks_window(dispatched_txs, 1, 6);
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

            Ok(Report {
                mailbox_data: MailboxReport {
                    removed: BTreeSet::from_iter(removed_from_mailbox),
                    ..Default::default()
                },
                ..Default::default()
            })
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
            process_events(
                api,
                messages,
                rx,
                block_number,
                mid_map,
                wait_for_event_blocks,
                BTreeMap::new(),
            )
            .await
            .map(|mut report| {
                report.mailbox_data.append_removed(removed_from_mailbox);
                report
            })
        }

        Batch::CreateProgram(args) => {
            tracing::info!(programs = args.len(), "Creating programs");
            let mut programs = BTreeSet::new();
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

            for (call_id, program_id, message_id) in created {
                programs.insert(program_id);
                mid_map.write().await.insert(message_id, program_id);
                messages.insert(message_id, (program_id, call_id));
                tracing::trace!(call_id, %program_id, %message_id, "Program created");
            }

            let wait_for_event_blocks = blocks_window(tx_count, 1, 6);
            process_events(
                api,
                messages,
                rx,
                block_number,
                mid_map,
                wait_for_event_blocks,
                BTreeMap::new(),
            )
            .await
        }
    }
}

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

fn blocks_window(action_count: usize, blocks_per_action: usize, headroom_blocks: usize) -> usize {
    action_count
        .saturating_mul(blocks_per_action)
        .saturating_add(headroom_blocks)
}

#[allow(clippy::too_many_arguments)]
async fn parse_router_transitions(
    api: &Ethereum,
    current_bn: FixedBytes<32>,
    to: Address,
    sent_message_ids: &BTreeSet<MessageId>,
    mid_map: &MidMap,
    mailbox_added: &mut BTreeSet<MessageId>,
    exited_programs: &mut BTreeSet<ActorId>,
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

                                if tr.exited && exited_programs.insert(actor_id) {
                                    tracing::debug!(program = %actor_id, "Program exited");
                                }

                                {
                                    let mut lock = mid_map.write().await;
                                    for vc in tr.valueClaims.iter() {
                                        lock.insert(MessageId::new(vc.messageId.0), actor_id);
                                    }
                                }

                                for msg in tr.messages.iter() {
                                    let msg_id = MessageId::new(msg.id.0);

                                    mid_map.write().await.insert(msg_id, actor_id);

                                    let is_reply = msg.replyDetails.to.0 != [0u8; 32];
                                    if msg.destination == to {
                                        mailbox_added.insert(msg_id);
                                    }

                                    if is_reply {
                                        let replied_to = MessageId::new(msg.replyDetails.to.0);

                                        {
                                            let mut lock = mid_map.write().await;
                                            lock.insert(replied_to, actor_id);
                                            lock.insert(
                                                MessageId::generate_reply(replied_to),
                                                actor_id,
                                            );
                                        }

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

async fn parse_mirror_logs(
    api: &Ethereum,
    current_bn: FixedBytes<32>,
    mid_map: &MidMap,
    events: &mut Vec<Event>,
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

            events.push(event);
        }
    }

    Ok(())
}

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
async fn process_events(
    api: Ethereum,
    mut messages: BTreeMap<MessageId, (ActorId, usize)>,
    mut rx: Receiver<Header>,
    block_number: u64,
    mid_map: MidMap,
    wait_for_event_blocks: usize,
    injected_promises: BTreeMap<MessageId, Promise>,
) -> Result<Report> {
    let mut mailbox_added = BTreeSet::new();
    let mut exited_programs = BTreeSet::new();
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
                &mid_map,
                &mut mailbox_added,
                &mut exited_programs,
                &mut transition_outcomes,
            )
            .await?;

            parse_mirror_logs(&api, current_bn, &mid_map, &mut v).await?;

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

    let mut program_ids = BTreeSet::new();
    for (mid, maybe_err) in &results {
        if let Some((pid, call_id)) = messages.remove(mid) {
            if let Some(expl) = maybe_err {
                tracing::debug!(call_id, %pid, %mid, error = %expl, "Call failed");
            } else {
                tracing::debug!(call_id, %pid, %mid, "Call succeeded");
                program_ids.insert(pid);
            }
        }
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

    Ok(Report {
        program_ids,
        mailbox_data: mailbox_added.into(),
        exited_programs,
        ..Default::default()
    })
}
