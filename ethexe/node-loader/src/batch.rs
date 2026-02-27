use alloy::{
    consensus::Transaction,
    eips::BlockId,
    network::{BlockResponse, Network, primitives::HeaderResponse},
    primitives::{Address, FixedBytes, U256},
    providers::{Provider, WalletProvider},
    rpc::types::{BlockTransactions, Filter},
    sol_types::{SolCall, SolEvent},
};
use anyhow::Result;
use ethexe_common::{Address as EthexeAddress, events::MirrorEvent};
use ethexe_ethereum::{
    Ethereum, TryGetReceipt,
    abi::{IMirror, IRouter::commitBatchCall},
    mirror::events::try_extract_event,
};
use ethexe_sdk::VaraEthApi;
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

alloy::sol!(
    #[sol(rpc)]
    BatchMulticall,
    "BatchMulticall.json"
);

pub async fn deploy_send_message_multicall(api: &Ethereum) -> Result<Address> {
    let multicall = BatchMulticall::deploy(api.provider()).await?;

    Ok(*multicall.address())
}

pub struct BatchPool<Rng: CallGenRng> {
    apis: Vec<Ethereum>,
    eth_rpc_url: String,
    pool_size: usize,
    batch_size: usize,
    send_message_multicall: Address,
    task_context: Context,
    rx: Receiver<<alloy::network::Ethereum as Network>::HeaderResponse>,
    _marker: PhantomData<Rng>,
}

type MidMap = Arc<RwLock<BTreeMap<MessageId, ActorId>>>;

#[derive(Debug, Default, Clone, Copy)]
struct ProcessEventsStats {
    start_block_found: bool,
    start_search_window_blocks: usize,

    router_txs_seen: usize,
    commit_batch_calls_decoded: usize,
    chain_commitments_seen: usize,
    transitions_seen: usize,
    transition_messages_seen: usize,
    transition_value_claims_seen: usize,
    transition_reply_details_seen: usize,
    transition_replies_matched: usize,
    transition_mailbox_added: usize,
    transition_exited_programs: usize,

    mirror_logs_seen: usize,
    mirror_events_decoded: usize,
    mirror_message_events: usize,
    mirror_reply_events: usize,
    mirror_call_failed_events: usize,
    mirror_value_claimed_events: usize,
}

const INJECTED_TX_RATIO_NUM: u8 = 7;
const INJECTED_TX_RATIO_DEN: u8 = 10;
/// This is the amount of VARA to top up newly created programs with.
///
/// It is an ERC20 token with 12 decimals, so this is 500,000 VARA.
const TOP_UP_AMOUNT: u128 = 500_000_000_000_000;

fn prefer_injected_tx(rng: &mut impl RngCore) -> bool {
    // Make injected txs common, but still keep some on-chain `send_message` calls.
    (rng.next_u32() % INJECTED_TX_RATIO_DEN as u32) < INJECTED_TX_RATIO_NUM as u32
}

/// Generate a fuzzed value for a message.
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
        eth_rpc_url: String,
        pool_size: usize,
        batch_size: usize,
        send_message_multicall: Address,
        rx: Receiver<<alloy::network::Ethereum as Network>::HeaderResponse>,
    ) -> Self {
        Self {
            apis,
            eth_rpc_url,
            pool_size,
            batch_size,
            send_message_multicall,
            task_context: Context::new(),
            rx,
            _marker: PhantomData,
        }
    }

    pub async fn run(
        mut self,
        params: LoadParams,
        _rx: Receiver<<alloy::network::Ethereum as Network>::HeaderResponse>,
    ) -> Result<()> {
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
            let vapi = VaraEthApi::new(&self.eth_rpc_url, api.clone()).await?;
            batches.push(run_batch_for_worker(
                worker_idx,
                api,
                vapi,
                batch_with_seed,
                self.send_message_multicall,
                self.rx.resubscribe(),
                mid_map.clone(),
            ));
        }

        while let Some((worker_idx, report)) = batches.next().await {
            self.process_run_report(report?);

            let batch_with_seed = batch_gen.generate(self.task_context.clone());
            let api = self.apis[worker_idx].clone();
            let vapi = VaraEthApi::new(&self.eth_rpc_url, api.clone()).await?;
            batches.push(run_batch_for_worker(
                worker_idx,
                api,
                vapi,
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
    vapi: VaraEthApi,
    batch: BatchWithSeed,
    send_message_multicall: Address,
    rx: Receiver<<alloy::network::Ethereum as Network>::HeaderResponse>,
    mid_map: MidMap,
) -> Result<BatchRunReport> {
    let (seed, batch) = batch.into();
    let mut rng = SmallRng::seed_from_u64(seed);

    match run_batch_impl(
        api,
        vapi,
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
    }
}

async fn run_batch_for_worker(
    worker_idx: usize,
    api: Ethereum,
    vapi: VaraEthApi,
    batch: BatchWithSeed,
    send_message_multicall: Address,
    rx: Receiver<<alloy::network::Ethereum as Network>::HeaderResponse>,
    mid_map: MidMap,
) -> (usize, Result<BatchRunReport>) {
    (
        worker_idx,
        run_batch(api, vapi, batch, send_message_multicall, rx, mid_map).await,
    )
}

#[instrument(skip_all)]
async fn run_batch_impl(
    api: Ethereum,
    vapi: VaraEthApi,
    batch: Batch,
    send_message_multicall: Address,
    rx: Receiver<<alloy::network::Ethereum as Network>::HeaderResponse>,
    mid_map: MidMap,
    rng: &mut SmallRng,
) -> Result<Report> {
    match batch {
        Batch::UploadProgram(args) => {
            tracing::info!("Uploading programs");
            let mut code_ids = Vec::with_capacity(args.len());

            for arg in args.iter() {
                let expected_code_id = CodeId::generate(&arg.0.0);
                tracing::debug!(
                    "Uploading code {} for program (len = {} bytes)",
                    expected_code_id,
                    arg.0.0.len()
                );
                let (_, code_id) = vapi.router().request_code_validation(&arg.0.0).await?;
                tracing::debug!("Code {code_id} upload requested");
                assert_eq!(code_id, CodeId::generate(&arg.0.0));
                vapi.router().wait_for_code_validation(code_id).await?;
                tracing::debug!("Code {code_id} uploaded and validated");
                code_ids.push(code_id);
            }

            let mut program_ids = BTreeSet::new();
            let mut messages = BTreeMap::new();
            let block_number = api.provider().get_block_number().await?;
            for (call_id, (arg, code_id)) in args.iter().zip(code_ids.iter().copied()).enumerate() {
                let salt = &arg.0.1;
                let (_, program_id) = api
                    .router()
                    .create_program(code_id, salt_to_h256(salt), None)
                    .await?;

                api.router()
                    .wvara()
                    .approve(program_id, TOP_UP_AMOUNT)
                    .await?;
                let mirror = api.mirror(program_id);
                mirror.executable_balance_top_up(TOP_UP_AMOUNT).await?;
                tracing::debug!("[Call with id {call_id}]: Program created {program_id}");

                let fuzzed_value = fuzz_message_value(rng);
                tracing::debug!(
                    "[Call with id {call_id}]: Sending init message to {program_id} with value={fuzzed_value}"
                );
                let mirror = api.mirror(program_id);
                let (_, message_id) = mirror.send_message(&arg.0.2, fuzzed_value).await?;
                program_ids.insert(program_id);
                mid_map.write().await.insert(message_id, program_id);
                messages.insert(message_id, (program_id, call_id));
                tracing::debug!(
                    "[Call with id {call_id}]: Program created {program_id}, init sent {message_id}"
                );
            }

            let blocks_per_action = 4;
            let wait_for_event_blocks = blocks_window(args.len(), blocks_per_action, 4);
            process_events(
                api,
                messages,
                rx,
                block_number,
                mid_map,
                wait_for_event_blocks,
            )
            .await
        }

        Batch::UploadCode(args) => {
            tracing::info!("Uploading codes");
            let mut code_ids = Vec::with_capacity(args.len());
            let start = std::time::Instant::now();

            for arg in args.iter() {
                let expected_code_id = CodeId::generate(&arg.0);
                tracing::debug!("Uploading code {expected_code_id} (len = {})", arg.0.len());
                let (_, code_id) = vapi.router().request_code_validation(&arg.0).await?;
                tracing::debug!("Code {code_id} upload requested");
                assert_eq!(code_id, CodeId::generate(&arg.0));
                vapi.router().wait_for_code_validation(code_id).await?;
                tracing::debug!("Code {code_id} uploaded and validated");
                code_ids.push(code_id);
            }

            tracing::debug!(
                "Validated {} code(s) in {:?}s",
                code_ids.len(),
                start.elapsed().as_secs_f64()
            );

            Ok(Report {
                codes: code_ids.into_iter().collect(),
                ..Default::default()
            })
        }

        Batch::SendMessage(args) => {
            tracing::info!("Sending messages");
            let mut messages = BTreeMap::new();
            let block_number = api.provider().get_block_number().await?;
            let mut regular_calls = Vec::new();

            for (i, arg) in args.iter().enumerate() {
                let to = arg.0.0;
                let fuzzed_value = fuzz_message_value(rng);
                if prefer_injected_tx(rng) {
                    tracing::debug!(
                        "[Call with id {i}]: Sending injected message to {to} with value=0"
                    );
                    let mirror = vapi.mirror(to);
                    let message_id = mirror.send_message_injected(&arg.0.1, 0).await?;
                    messages.insert(message_id, (to, i));
                    mid_map.write().await.insert(message_id, to);
                    tracing::debug!("[Call with id {i}]: Message sent #{message_id} to {to}");
                } else {
                    tracing::debug!(
                        "[Call with id {i}]: Queuing message to {to} via multicall with value={fuzzed_value}"
                    );
                    regular_calls.push((i, to, arg.0.1.clone(), fuzzed_value));
                }
            }

            if !regular_calls.is_empty() {
                let sent =
                    send_message_batch_via_multicall(&api, send_message_multicall, &regular_calls)
                        .await?;

                for (call_id, to, message_id) in sent {
                    messages.insert(message_id, (to, call_id));
                    mid_map.write().await.insert(message_id, to);
                    tracing::debug!("[Call with id {call_id}]: Message sent #{message_id} to {to}");
                }
            }

            let blocks_per_action = 1;
            let wait_for_event_blocks = blocks_window(args.len(), blocks_per_action, 4);
            process_events(
                api,
                messages,
                rx,
                block_number,
                mid_map,
                wait_for_event_blocks,
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
                tracing::debug!("[Call with id: {call_id}]: Successfully claimed");
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
                // Mirror emits `Reply(..., replyTo=mid, ...)`, so track the original id.
                messages.insert(mid, (actor_id, call_id));

                tracing::debug!(
                    "[Call with id: {call_id}]: Successfully replied to {mid} with value={fuzzed_value}"
                );
            }

            let blocks_per_action = 1;
            let wait_for_event_blocks = blocks_window(args.len(), blocks_per_action, 4);
            process_events(
                api,
                messages,
                rx,
                block_number,
                mid_map,
                wait_for_event_blocks,
            )
            .await
            .map(|mut report| {
                report.mailbox_data.append_removed(removed_from_mailbox);
                report
            })
        }

        Batch::CreateProgram(args) => {
            tracing::info!("Creating programs");
            let mut programs = BTreeSet::new();
            let mut messages = BTreeMap::new();
            let block_number = api.provider().get_block_number().await?;

            for (call_id, arg) in args.iter().enumerate() {
                let code_id = arg.0.0;
                let salt = &arg.0.1;
                let (_, program_id) = api
                    .router()
                    .create_program(code_id, salt_to_h256(salt), None)
                    .await?;
                api.router()
                    .wvara()
                    .approve(program_id, TOP_UP_AMOUNT)
                    .await?;
                let mirror = api.mirror(program_id);
                mirror.executable_balance_top_up(TOP_UP_AMOUNT).await?;
                tracing::debug!("[Call with id: {call_id}]: Program created {program_id}");

                let fuzzed_value = fuzz_message_value(rng);
                tracing::debug!(
                    "[Call with id: {call_id}]: Sending init message to {program_id} with value={fuzzed_value}",
                );
                let mirror = api.mirror(program_id);
                let (_, message_id) = mirror.send_message(&arg.0.2, fuzzed_value).await?;
                programs.insert(program_id);
                mid_map.write().await.insert(message_id, program_id);
                messages.insert(message_id, (program_id, call_id));
                tracing::debug!(
                    "[Call with id: {call_id}]: Successfully sent init message {message_id}"
                );
            }

            let blocks_per_action = 4;
            let wait_for_event_blocks = blocks_window(args.len(), blocks_per_action, 4);
            process_events(
                api,
                messages,
                rx,
                block_number,
                mid_map,
                wait_for_event_blocks,
            )
            .await
        }
    }
}

async fn send_message_batch_via_multicall(
    api: &Ethereum,
    multicall_address: Address,
    calls: &[(usize, ActorId, Vec<u8>, u128)],
) -> Result<Vec<(usize, ActorId, MessageId)>> {
    let multicall = BatchMulticall::new(multicall_address, api.provider());

    let mut value_sum = 0_u128;
    let mut batched_calls = Vec::with_capacity(calls.len());
    for (_, actor_id, payload, value) in calls {
        value_sum = value_sum.saturating_add(*value);
        batched_calls.push(BatchMulticall::MessageCall {
            mirror: Address::from(actor_id.to_address_lossy().0),
            payload: payload.clone().into(),
            value: *value,
        });
    }

    let receipt = multicall
        .sendMessageBatch(batched_calls)
        .value(U256::from(value_sum))
        .send()
        .await?
        .try_get_receipt_check_reverted()
        .await?;

    let mut message_ids = Vec::with_capacity(calls.len());
    for log in receipt.inner.logs() {
        if log.topic0() == Some(&ethexe_ethereum::mirror::signatures::MESSAGE_QUEUEING_REQUESTED) {
            let event =
                IMirror::MessageQueueingRequested::decode_raw_log(log.topics(), &log.data().data)?;
            message_ids.push((*event.id).into());
        }
    }

    if message_ids.len() != calls.len() {
        tracing::warn!(
            expected = calls.len(),
            actual = message_ids.len(),
            "multicall send_message produced fewer message events than requested calls"
        );
    }

    let mapped = calls
        .iter()
        .zip(message_ids)
        .map(|((call_id, to, ..), mid)| (*call_id, *to, mid))
        .collect();

    Ok(mapped)
}

fn blocks_window(action_count: usize, blocks_per_action: usize, headroom_blocks: usize) -> usize {
    action_count
        .saturating_mul(blocks_per_action)
        .saturating_add(headroom_blocks)
}

#[derive(Debug, Default, Clone, Copy)]
struct BlockProcessStats {
    router_txs_seen: usize,
    commit_batch_calls_decoded: usize,
    chain_commitments_seen: usize,
    transitions_seen: usize,
    transition_messages_seen: usize,
    transition_value_claims_seen: usize,
    transition_reply_details_seen: usize,
    transition_replies_matched: usize,
    transition_mailbox_added: usize,
    transition_exited_programs: usize,

    mirror_logs_seen: usize,
    mirror_events_decoded: usize,
    mirror_message_events: usize,
    mirror_reply_events: usize,
    mirror_call_failed_events: usize,
    mirror_value_claimed_events: usize,
}

impl ProcessEventsStats {
    fn add_block(&mut self, block: BlockProcessStats) {
        self.router_txs_seen = self.router_txs_seen.saturating_add(block.router_txs_seen);
        self.commit_batch_calls_decoded = self
            .commit_batch_calls_decoded
            .saturating_add(block.commit_batch_calls_decoded);
        self.chain_commitments_seen = self
            .chain_commitments_seen
            .saturating_add(block.chain_commitments_seen);
        self.transitions_seen = self.transitions_seen.saturating_add(block.transitions_seen);
        self.transition_messages_seen = self
            .transition_messages_seen
            .saturating_add(block.transition_messages_seen);
        self.transition_value_claims_seen = self
            .transition_value_claims_seen
            .saturating_add(block.transition_value_claims_seen);
        self.transition_reply_details_seen = self
            .transition_reply_details_seen
            .saturating_add(block.transition_reply_details_seen);
        self.transition_replies_matched = self
            .transition_replies_matched
            .saturating_add(block.transition_replies_matched);
        self.transition_mailbox_added = self
            .transition_mailbox_added
            .saturating_add(block.transition_mailbox_added);
        self.transition_exited_programs = self
            .transition_exited_programs
            .saturating_add(block.transition_exited_programs);

        self.mirror_logs_seen = self.mirror_logs_seen.saturating_add(block.mirror_logs_seen);
        self.mirror_events_decoded = self
            .mirror_events_decoded
            .saturating_add(block.mirror_events_decoded);
        self.mirror_message_events = self
            .mirror_message_events
            .saturating_add(block.mirror_message_events);
        self.mirror_reply_events = self
            .mirror_reply_events
            .saturating_add(block.mirror_reply_events);
        self.mirror_call_failed_events = self
            .mirror_call_failed_events
            .saturating_add(block.mirror_call_failed_events);
        self.mirror_value_claimed_events = self
            .mirror_value_claimed_events
            .saturating_add(block.mirror_value_claimed_events);
    }
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
) -> Result<BlockProcessStats> {
    let mut block_stats = BlockProcessStats::default();

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
                block_stats.router_txs_seen += 1;
                match commitBatchCall::abi_decode(tx.input()) {
                    Ok(commit_batch) => {
                        block_stats.commit_batch_calls_decoded += 1;
                        let batch = commit_batch._batch;
                        tracing::debug!(
                            block_hash = ?current_bn,
                            chain_commitments = batch.chainCommitment.len(),
                            "Decoded Router.commitBatch calldata"
                        );
                        for commitment in batch.chainCommitment.iter() {
                            block_stats.chain_commitments_seen += 1;
                            for tr in commitment.transitions.iter() {
                                block_stats.transitions_seen += 1;
                                let actor_id: ActorId = EthexeAddress::from(tr.actorId).into();

                                if tr.exited {
                                    if exited_programs.insert(actor_id) {
                                        block_stats.transition_exited_programs += 1;
                                    }
                                    tracing::debug!(
                                        block_hash = ?current_bn,
                                        program = ?actor_id,
                                        "Program exited"
                                    );
                                }

                                {
                                    let mut lock = mid_map.write().await;
                                    for vc in tr.valueClaims.iter() {
                                        block_stats.transition_value_claims_seen += 1;
                                        lock.insert(MessageId::new(vc.messageId.0), actor_id);
                                    }
                                }

                                for msg in tr.messages.iter() {
                                    block_stats.transition_messages_seen += 1;
                                    let msg_id = MessageId::new(msg.id.0);

                                    mid_map.write().await.insert(msg_id, actor_id);

                                    let is_reply = msg.replyDetails.to.0 != [0u8; 32];
                                    if msg.destination == to && !is_reply {
                                        mailbox_added.insert(msg_id);
                                        block_stats.transition_mailbox_added += 1;
                                    }

                                    if is_reply {
                                        block_stats.transition_reply_details_seen += 1;
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
                                            block_stats.transition_replies_matched += 1;
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

                                            tracing::debug!(
                                                block_hash = ?current_bn,
                                                program = ?actor_id,
                                                replied_to = ?replied_to,
                                                reply_code = ?reply_code,
                                                "Matched reply outcome from Router transitions"
                                            );
                                        } else {
                                            tracing::trace!(
                                                block_hash = ?current_bn,
                                                program = ?actor_id,
                                                msg_id = ?msg_id,
                                                replied_to = ?replied_to,
                                                "ReplyDetails present in transition, but replyTo isn't tracked by this batch"
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

    Ok(block_stats)
}

async fn parse_mirror_logs(
    api: &Ethereum,
    current_bn: FixedBytes<32>,
    mid_map: &MidMap,
    events: &mut Vec<Event>,
) -> Result<BlockProcessStats> {
    let mut block_stats = BlockProcessStats::default();
    let logs = api
        .provider()
        .get_logs(&Filter::new().at_block_hash(current_bn))
        .await?;

    block_stats.mirror_logs_seen = logs.len();

    for log in logs {
        if let Some(mirror_event) = try_extract_event(&log)? {
            let actor_id: ActorId = EthexeAddress::from(log.address()).into();
            let event = Event {
                event: mirror_event,
                actor_id,
            };
            tracing::debug!("Relevant log discovered: {event:?}");

            block_stats.mirror_events_decoded += 1;
            match &event.event {
                MirrorEvent::Message(_) => block_stats.mirror_message_events += 1,
                MirrorEvent::Reply(_) => block_stats.mirror_reply_events += 1,
                MirrorEvent::MessageCallFailed(_) | MirrorEvent::ReplyCallFailed(_) => {
                    block_stats.mirror_call_failed_events += 1;
                }
                MirrorEvent::ValueClaimed(_) => block_stats.mirror_value_claimed_events += 1,
                _ => {}
            }

            {
                let mut lock = mid_map.write().await;
                match &event.event {
                    MirrorEvent::MessageQueueingRequested(ev) => {
                        lock.insert(ev.id, actor_id);
                    }
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

    Ok(block_stats)
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
async fn process_events(
    api: Ethereum,
    mut messages: BTreeMap<MessageId, (ActorId, usize)>,
    mut rx: Receiver<<alloy::network::Ethereum as Network>::HeaderResponse>,
    block_number: u64,
    mid_map: MidMap,
    wait_for_event_blocks: usize,
) -> Result<Report> {
    let mut mailbox_added = BTreeSet::new();
    let mut exited_programs = BTreeSet::new();
    let initial_messages_len = messages.len();
    let mut stats = ProcessEventsStats {
        start_search_window_blocks: 0,
        ..Default::default()
    };

    let results = {
        let mut block = recv_next_header(&mut rx).await?;
        while block.number < block_number {
            stats.start_search_window_blocks += 1;
            block = recv_next_header(&mut rx).await?;
        }
        stats.start_block_found = true;
        tracing::info!(
            target_block = block_number,
            observed_block = block.number,
            "Start block reached after searching {} blocks",
            stats.start_search_window_blocks
        );
        let to: Address = api.provider().default_signer_address();
        let sent_message_ids: BTreeSet<MessageId> = messages.keys().copied().collect();
        let mut transition_outcomes: BTreeMap<MessageId, Option<String>> = BTreeMap::new();

        let mut v = Vec::new();
        let mut current_bn = block.hash();
        for _ in 0..wait_for_event_blocks {
            // Parse Router commitBatch calldata for this block and merge with Mirror logs.
            // This is particularly important for injected transactions where Mirror request logs
            // might not be present, but transitions still contain the canonical reply.
            let transition_stats = parse_router_transitions(
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

            tracing::debug!(
                block_hash = ?current_bn,
                router_txs_seen = transition_stats.router_txs_seen,
                commit_batch_calls_decoded = transition_stats.commit_batch_calls_decoded,
                chain_commitments_seen = transition_stats.chain_commitments_seen,
                transitions_seen = transition_stats.transitions_seen,
                transition_messages_seen = transition_stats.transition_messages_seen,
                transition_value_claims_seen = transition_stats.transition_value_claims_seen,
                transition_reply_details_seen = transition_stats.transition_reply_details_seen,
                transition_replies_matched = transition_stats.transition_replies_matched,
                transition_mailbox_added = transition_stats.transition_mailbox_added,
                transition_exited_programs = transition_stats.transition_exited_programs,
                "Router transition parse summary"
            );

            let mirror_stats = parse_mirror_logs(&api, current_bn, &mid_map, &mut v).await?;

            stats.add_block(BlockProcessStats {
                router_txs_seen: transition_stats.router_txs_seen,
                commit_batch_calls_decoded: transition_stats.commit_batch_calls_decoded,
                chain_commitments_seen: transition_stats.chain_commitments_seen,
                transitions_seen: transition_stats.transitions_seen,
                transition_messages_seen: transition_stats.transition_messages_seen,
                transition_value_claims_seen: transition_stats.transition_value_claims_seen,
                transition_reply_details_seen: transition_stats.transition_reply_details_seen,
                transition_replies_matched: transition_stats.transition_replies_matched,
                transition_mailbox_added: transition_stats.transition_mailbox_added,
                transition_exited_programs: transition_stats.transition_exited_programs,
                mirror_logs_seen: mirror_stats.mirror_logs_seen,
                mirror_events_decoded: mirror_stats.mirror_events_decoded,
                mirror_message_events: mirror_stats.mirror_message_events,
                mirror_reply_events: mirror_stats.mirror_reply_events,
                mirror_call_failed_events: mirror_stats.mirror_call_failed_events,
                mirror_value_claimed_events: mirror_stats.mirror_value_claimed_events,
            });

            let mut mailbox_from_events =
                utils::capture_mailbox_messages(&api, &v, messages.keys().copied()).await?;
            mailbox_added.append(&mut mailbox_from_events);

            block = recv_next_header(&mut rx).await?;
            current_bn = block.hash();
        }

        let mut result_map: BTreeMap<MessageId, Option<String>> = BTreeMap::new();

        for (mid, status) in
            utils::err_waited_or_succeed_batch(&mut v, messages.keys().copied()).await
        {
            result_map.insert(mid, status);
        }

        // Merge transition-derived outcomes with log-derived outcomes.
        for (mid, status) in transition_outcomes {
            let entry = result_map.entry(mid).or_insert(Some("UNKNOWN".to_string()));
            match (&entry, &status) {
                (Some(current), None) if current == "UNKNOWN" => *entry = None,
                (None, Some(_)) => *entry = status,
                (Some(current), Some(_)) if current == "UNKNOWN" => *entry = status,
                _ => {}
            }
        }

        // Gear node-loader reports UNKNOWN when no terminal outcome is observed
        // inside the event window; mirror that behavior here.
        if !messages.is_empty() {
            let resolved: BTreeSet<MessageId> = result_map.keys().copied().collect();
            for mid in messages.keys().copied() {
                if !resolved.contains(&mid) {
                    result_map.insert(mid, Some("UNKNOWN".to_string()));
                }
            }
        }

        result_map.into_iter().collect::<Vec<_>>()
    };

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
        if messages.is_empty() {
            break;
        }

        if let Some((pid, call_id)) = messages.remove(mid) {
            if let Some(expl) = maybe_err {
                tracing::debug!(
                    "[Call with id: {call_id}]: {mid:#.2} executing within program '{pid:#.2}' ended with a trap: '{expl}'"
                );
            } else {
                tracing::debug!(
                    "[Call with id: {call_id}]: {mid:#.2} executing within program '{pid:#.2}' ended successfully"
                );
                program_ids.insert(pid);
            }
        }
    }

    if !messages.is_empty() {
        tracing::error!("Unresolved messages: {messages:?}");
    }

    let unresolved_count = messages.len();
    let unresolved_sample: Vec<MessageId> = messages.keys().copied().take(10).collect();
    tracing::info!(
        start_block_target = block_number,
        wait_for_event_blocks,
        batch_messages_total = initial_messages_len,
        results_total = results.len(),
        results_ok = ok_count,
        results_err = err_count,
        results_unknown = unknown_count,
        mailbox_added = mailbox_added.len(),
        program_ids = program_ids.len(),
        exited_programs = exited_programs.len(),
        unresolved_count,
        unresolved_sample = ?unresolved_sample,
        start_block_found = stats.start_block_found,
        start_search_window_blocks = stats.start_search_window_blocks,
        router_txs_seen = stats.router_txs_seen,
        commit_batch_calls_decoded = stats.commit_batch_calls_decoded,
        chain_commitments_seen = stats.chain_commitments_seen,
        transitions_seen = stats.transitions_seen,
        transition_messages_seen = stats.transition_messages_seen,
        transition_value_claims_seen = stats.transition_value_claims_seen,
        transition_reply_details_seen = stats.transition_reply_details_seen,
        transition_replies_matched = stats.transition_replies_matched,
        transition_mailbox_added = stats.transition_mailbox_added,
        transition_exited_programs = stats.transition_exited_programs,
        mirror_logs_seen = stats.mirror_logs_seen,
        mirror_events_decoded = stats.mirror_events_decoded,
        mirror_message_events = stats.mirror_message_events,
        mirror_reply_events = stats.mirror_reply_events,
        mirror_call_failed_events = stats.mirror_call_failed_events,
        mirror_value_claimed_events = stats.mirror_value_claimed_events,
        "process_events summary"
    );

    tracing::debug!("Mailbox {:?}", mailbox_added);
    Ok(Report {
        program_ids,
        mailbox_data: mailbox_added.into(),
        exited_programs,
        ..Default::default()
    })
}
