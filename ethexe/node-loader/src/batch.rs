use alloy::{
    consensus::Transaction,
    eips::BlockId,
    network::{BlockResponse, Network, primitives::HeaderResponse},
    primitives::{Address, FixedBytes},
    providers::{Provider, WalletProvider},
    rpc::types::{BlockTransactions, Filter},
    sol_types::SolCall,
};
use anyhow::Result;
use ethexe_common::{Address as EthexeAddress, events::MirrorEvent};
use ethexe_ethereum::{Ethereum, abi::IRouter::commitBatchCall, mirror::events::try_extract_event};
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
use tokio::sync::{RwLock, broadcast::Receiver};
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

pub struct BatchPool<Rng: CallGenRng> {
    apis: Vec<Ethereum>,
    eth_rpc_url: String,
    pool_size: usize,
    batch_size: usize,
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

    mirror_logs_seen: usize,
    mirror_events_decoded: usize,
    mirror_message_events: usize,
    mirror_reply_events: usize,
    mirror_call_failed_events: usize,
    mirror_value_claimed_events: usize,
}

const INJECTED_TX_RATIO_NUM: u8 = 7;
const INJECTED_TX_RATIO_DEN: u8 = 10;
const TOP_UP_AMOUNT: u128 = 500_000_000_000_000;

fn prefer_injected_tx(rng: &mut impl RngCore) -> bool {
    // Make injected txs common, but still keep some on-chain `send_message` calls.
    (rng.next_u32() % INJECTED_TX_RATIO_DEN as u32) < INJECTED_TX_RATIO_NUM as u32
}

fn salt_to_h256(salt: &[u8]) -> H256 {
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
        rx: Receiver<<alloy::network::Ethereum as Network>::HeaderResponse>,
    ) -> Self {
        Self {
            apis,
            eth_rpc_url,
            pool_size,
            batch_size,
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
    rx: Receiver<<alloy::network::Ethereum as Network>::HeaderResponse>,
    mid_map: MidMap,
) -> Result<BatchRunReport> {
    let (seed, batch) = batch.into();
    let mut rng = SmallRng::seed_from_u64(seed);

    match run_batch_impl(api, vapi, batch, rx, mid_map, &mut rng).await {
        Ok(report) => Ok(BatchRunReport::new(seed, report)),
        Err(err) => {
            tracing::info!("Batch failed: {err:?}");
            Ok(BatchRunReport::empty(seed))
        }
    }
}

async fn run_batch_for_worker(
    worker_idx: usize,
    api: Ethereum,
    vapi: VaraEthApi,
    batch: BatchWithSeed,
    rx: Receiver<<alloy::network::Ethereum as Network>::HeaderResponse>,
    mid_map: MidMap,
) -> (usize, Result<BatchRunReport>) {
    (worker_idx, run_batch(api, vapi, batch, rx, mid_map).await)
}

#[instrument(skip_all)]
async fn run_batch_impl(
    api: Ethereum,
    vapi: VaraEthApi,
    batch: Batch,
    rx: Receiver<<alloy::network::Ethereum as Network>::HeaderResponse>,
    mid_map: MidMap,
    rng: &mut SmallRng,
) -> Result<Report> {
    match batch {
        Batch::UploadProgram(args) => {
            tracing::info!("Uploading programs");
            let mut code_ids = Vec::with_capacity(args.len());

            for arg in args.iter() {
                tracing::debug!(
                    "Uploading code {} for program (len = {} bytes)",
                    CodeId::generate(&arg.0.0),
                    arg.0.0.len()
                );
                let (_, code_id) = vapi.router().request_code_validation(&arg.0.0).await?;
                vapi.router().wait_for_code_validation(code_id).await?;
                tracing::debug!("Code {code_id} uploaded and validated");
                code_ids.push(code_id);
            }

            let mut program_ids = BTreeSet::new();
            let mut messages = BTreeMap::new();
            let block = api
                .provider()
                .get_block(BlockId::latest())
                .await?
                .expect("no block?");
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

                // Send init message: prefer injected transactions, but keep some
                // regular on-chain calls to exercise both paths.
                let message_id = if prefer_injected_tx(rng) {
                    tracing::debug!(
                        "[Call with id {call_id}]: Sending injected init message to {program_id}"
                    );
                    vapi.mirror(program_id)
                        .send_message_injected(&arg.0.2, arg.0.4)
                        .await?
                } else {
                    tracing::debug!(
                        "[Call with id {call_id}]: Sending init message to {program_id} through Mirror contract"
                    );
                    let mirror = api.mirror(program_id);
                    let (_, mid) = mirror.send_message(&arg.0.2, arg.0.4).await?;
                    mid
                };

                mid_map.write().await.insert(message_id, program_id);
                messages.insert(message_id, (program_id, call_id));
                tracing::debug!("[Call with id {call_id}]: Init message sent {message_id}");
                program_ids.insert(program_id);
            }

            let wait_for_event_blocks = blocks_window(args.len(), 6, 48);
            process_events(
                api,
                messages,
                rx,
                block.hash(),
                mid_map,
                wait_for_event_blocks,
            )
            .await
        }

        Batch::UploadCode(args) => {
            tracing::info!("Uploading codes");
            let mut code_ids = Vec::with_capacity(args.len());

            for arg in args.iter() {
                let code_id = CodeId::generate(&arg.0);
                tracing::debug!("Uploading code {code_id} (len = {})", arg.0.len());
                let start = std::time::Instant::now();
                let (_, code_id) = vapi.router().request_code_validation(&arg.0).await?;
                vapi.router().wait_for_code_validation(code_id).await?;
                tracing::debug!(
                    "Code {code_id} uploaded and validated in {:?}s",
                    start.elapsed().as_secs_f64()
                );
                code_ids.push(code_id);
            }

            Ok(Report {
                codes: code_ids.into_iter().collect(),
                ..Default::default()
            })
        }

        Batch::SendMessage(args) => {
            tracing::info!("Sending messages");
            let mut messages = BTreeMap::new();
            let block = api
                .provider()
                .get_block(BlockId::latest())
                .await?
                .unwrap()
                .hash();

            for (i, arg) in args.iter().enumerate() {
                let to = arg.0.0;
                let message_id = if prefer_injected_tx(rng) {
                    tracing::debug!("[Call with id {i}]: Sending injected message to {to}");
                    let mirror = vapi.mirror(to);
                    mirror.send_message_injected(&arg.0.1, arg.0.3).await?
                } else {
                    tracing::debug!(
                        "[Call with id {i}]: Sending message to {to} through Mirror contract"
                    );
                    let mirror = api.mirror(to);
                    let (_, mid) = mirror.send_message(&arg.0.1, arg.0.3).await?;
                    mid
                };
                messages.insert(message_id, (to, i));
                mid_map.write().await.insert(message_id, to);
                tracing::debug!("[Call with id {i}]: Message sent #{message_id} to {to}");
            }

            let wait_for_event_blocks = blocks_window(args.len(), 2, 16);
            process_events(api, messages, rx, block, mid_map, wait_for_event_blocks).await
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

            let block = api
                .provider()
                .get_block(BlockId::latest())
                .await?
                .unwrap()
                .hash();

            for (call_id, arg) in args.iter().enumerate() {
                let mid = arg.0.0;
                let payload = &arg.0.1;
                let value = arg.0.3;
                let actor_id = *mid_map
                    .read()
                    .await
                    .get(&mid)
                    .ok_or_else(|| anyhow::anyhow!("Actor not found for message id {mid}"))?;
                let mirror = api.mirror(actor_id);
                let _ = mirror.send_reply(mid, payload, value).await?;
                let reply_mid = MessageId::generate_reply(mid);
                mid_map.write().await.insert(reply_mid, actor_id);
                // Mirror emits `Reply(..., replyTo=mid, ...)`, so track the original id.
                messages.insert(mid, (actor_id, call_id));

                tracing::debug!(
                    "[Call with id: {call_id}]: Successfully replied to {mid} with value={value}"
                );
            }

            let wait_for_event_blocks = blocks_window(args.len(), 2, 16);
            process_events(api, messages, rx, block, mid_map, wait_for_event_blocks)
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
            let block_hash = api
                .provider()
                .get_block(BlockId::latest())
                .await?
                .expect("no block?");
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
                // send init message to program with payload and value.

                let message_id = if prefer_injected_tx(rng) {
                    tracing::debug!(
                        "[Call with id: {call_id}]: Sending injected init message to {program_id}"
                    );
                    vapi.mirror(program_id)
                        .send_message_injected(&arg.0.2, arg.0.4)
                        .await?
                } else {
                    tracing::debug!(
                        "[Call with id: {call_id}]: Sending init message to {program_id} through Mirror contract",
                    );
                    let (_, mid) = mirror.send_message(&arg.0.2, arg.0.4).await?;
                    mid
                };

                programs.insert(program_id);
                mid_map.write().await.insert(message_id, program_id);
                messages.insert(message_id, (program_id, call_id));
                tracing::debug!(
                    "[Call with id: {call_id}]: Successfully sent init message {message_id}"
                );
            }

            let wait_for_event_blocks = blocks_window(args.len(), 6, 48);
            process_events(
                api,
                messages,
                rx,
                block_hash.hash(),
                mid_map,
                wait_for_event_blocks,
            )
            .await
        }
    }
}

fn blocks_window(action_count: usize, blocks_per_action: usize, headroom_blocks: usize) -> usize {
    action_count
        .saturating_mul(blocks_per_action)
        .saturating_add(headroom_blocks)
        .max(10)
}

/// Wait for the new events since provided `block_hash`.
async fn process_events(
    api: Ethereum,
    mut messages: BTreeMap<MessageId, (ActorId, usize)>,
    mut rx: Receiver<<alloy::network::Ethereum as Network>::HeaderResponse>,
    block_hash: FixedBytes<32>,
    mid_map: MidMap,
    wait_for_event_blocks: usize,
) -> Result<Report> {
    let mut mailbox_added = BTreeSet::new();
    let initial_messages_len = messages.len();
    let mut stats = ProcessEventsStats {
        start_search_window_blocks: 5,
        ..Default::default()
    };

    let results = {
        let mut block = rx.recv().await?;
        let mut searched_blocks = 0usize;
        let start_search_window_blocks = stats.start_search_window_blocks;
        while block.hash() != block_hash && searched_blocks < start_search_window_blocks {
            block = rx.recv().await?;
            searched_blocks += 1;
        }

        if block.hash() != block_hash {
            tracing::debug!(
                "Start block hash wasn't observed within {start_search_window_blocks} blocks; starting from current block"
            );
            stats.start_block_found = false;
        } else {
            stats.start_block_found = true;
        }

        let to: Address = api.provider().default_signer_address();
        let sent_message_ids: BTreeSet<MessageId> = messages.keys().copied().collect();
        let mut transition_outcomes: BTreeMap<MessageId, Option<String>> = BTreeMap::new();

        let mut v = Vec::new();
        let mut current_bn = block.hash();
        for _ in 0..wait_for_event_blocks {
            let mut router_txs_seen = 0usize;
            let mut commit_batch_calls_decoded = 0usize;
            let mut chain_commitments_seen = 0usize;
            let mut transitions_seen = 0usize;
            let mut transition_messages_seen = 0usize;
            let mut transition_value_claims_seen = 0usize;
            let mut transition_reply_details_seen = 0usize;
            let mut transition_replies_matched = 0usize;
            let mut transition_mailbox_added = 0usize;

            // Parse Router commitBatch calldata for this block and merge with Mirror logs.
            // This is particularly important for injected transactions where Mirror request logs
            // might not be present, but transitions still contain the canonical reply.
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
                        router_txs_seen += 1;
                        match commitBatchCall::abi_decode(tx.input()) {
                            Ok(commit_batch) => {
                                commit_batch_calls_decoded += 1;
                                let batch = commit_batch._batch;
                                tracing::debug!(
                                    block_hash = ?current_bn,
                                    chain_commitments = batch.chainCommitment.len(),
                                    "Decoded Router.commitBatch calldata"
                                );
                                for commitment in batch.chainCommitment.iter() {
                                    chain_commitments_seen += 1;
                                    for tr in commitment.transitions.iter() {
                                        transitions_seen += 1;
                                        let actor_id: ActorId =
                                            EthexeAddress::from(tr.actorId).into();

                                        // Value claims help us keep `mid_map` warm for ClaimValue.
                                        {
                                            let mut lock = mid_map.write().await;
                                            for vc in tr.valueClaims.iter() {
                                                transition_value_claims_seen += 1;
                                                lock.insert(
                                                    MessageId::new(vc.messageId.0),
                                                    actor_id,
                                                );
                                            }
                                        }

                                        for msg in tr.messages.iter() {
                                            transition_messages_seen += 1;
                                            let msg_id = MessageId::new(msg.id.0);

                                            // Map message id to the emitting program.
                                            mid_map.write().await.insert(msg_id, actor_id);

                                            // Mailbox: messages delivered to our EOA (replies do not land in mailbox).
                                            let is_reply = msg.replyDetails.to.0 != [0u8; 32];
                                            if msg.destination == to && !is_reply {
                                                mailbox_added.insert(msg_id);
                                                transition_mailbox_added += 1;
                                            }

                                            // Reply detection: in transitions, a reply is encoded via replyDetails.
                                            // If this is a reply to one of our sent messages, we can derive an outcome.
                                            if is_reply {
                                                transition_reply_details_seen += 1;
                                                let replied_to =
                                                    MessageId::new(msg.replyDetails.to.0);

                                                // Keep additional mappings for later Reply/Claim flows.
                                                {
                                                    let mut lock = mid_map.write().await;
                                                    lock.insert(replied_to, actor_id);
                                                    lock.insert(
                                                        MessageId::generate_reply(replied_to),
                                                        actor_id,
                                                    );
                                                }

                                                if sent_message_ids.contains(&replied_to) {
                                                    transition_replies_matched += 1;
                                                    let reply_code = ReplyCode::from_bytes(
                                                        msg.replyDetails.code.0,
                                                    );
                                                    let err =
                                                        (!reply_code.is_success()).then(|| {
                                                            String::from_utf8(msg.payload.to_vec())
                                                                .unwrap_or_else(|_| {
                                                                    "<non-utf8 reply payload>"
                                                                        .to_string()
                                                                })
                                                        });

                                                    let entry = transition_outcomes
                                                        .entry(replied_to)
                                                        .or_insert(Some("UNKNOWN".to_string()));
                                                    match (&entry, &err) {
                                                        (Some(current), None)
                                                            if current == "UNKNOWN" =>
                                                        {
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

            tracing::debug!(
                block_hash = ?current_bn,
                router_txs_seen,
                commit_batch_calls_decoded,
                chain_commitments_seen,
                transitions_seen,
                transition_messages_seen,
                transition_value_claims_seen,
                transition_reply_details_seen,
                transition_replies_matched,
                transition_mailbox_added,
                "Router transition parse summary"
            );

            stats.router_txs_seen = stats.router_txs_seen.saturating_add(router_txs_seen);
            stats.commit_batch_calls_decoded = stats
                .commit_batch_calls_decoded
                .saturating_add(commit_batch_calls_decoded);
            stats.chain_commitments_seen = stats
                .chain_commitments_seen
                .saturating_add(chain_commitments_seen);
            stats.transitions_seen = stats.transitions_seen.saturating_add(transitions_seen);
            stats.transition_messages_seen = stats
                .transition_messages_seen
                .saturating_add(transition_messages_seen);
            stats.transition_value_claims_seen = stats
                .transition_value_claims_seen
                .saturating_add(transition_value_claims_seen);
            stats.transition_reply_details_seen = stats
                .transition_reply_details_seen
                .saturating_add(transition_reply_details_seen);
            stats.transition_replies_matched = stats
                .transition_replies_matched
                .saturating_add(transition_replies_matched);
            stats.transition_mailbox_added = stats
                .transition_mailbox_added
                .saturating_add(transition_mailbox_added);

            let logs = api
                .provider()
                .get_logs(&Filter::new().at_block_hash(current_bn))
                .await?;

            stats.mirror_logs_seen = stats.mirror_logs_seen.saturating_add(logs.len());

            for log in logs {
                if let Some(mirror_event) = try_extract_event(&log)? {
                    let actor_id: ActorId = EthexeAddress::from(log.address()).into();
                    let event = Event {
                        event: mirror_event,
                        actor_id,
                    };
                    tracing::debug!("Relevant log discovered: {event:?}");

                    stats.mirror_events_decoded = stats.mirror_events_decoded.saturating_add(1);
                    match &event.event {
                        MirrorEvent::Message(_) => {
                            stats.mirror_message_events =
                                stats.mirror_message_events.saturating_add(1);
                        }
                        MirrorEvent::Reply(_) => {
                            stats.mirror_reply_events = stats.mirror_reply_events.saturating_add(1);
                        }
                        MirrorEvent::MessageCallFailed(_) | MirrorEvent::ReplyCallFailed(_) => {
                            stats.mirror_call_failed_events =
                                stats.mirror_call_failed_events.saturating_add(1);
                        }
                        MirrorEvent::ValueClaimed(_) => {
                            stats.mirror_value_claimed_events =
                                stats.mirror_value_claimed_events.saturating_add(1);
                        }
                        _ => {}
                    }

                    // Enrich message->program map from emitted logs.
                    // The emitting contract address is the program mirror address.
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

                    v.push(event);
                }
            }

            let mut mailbox_from_events =
                utils::capture_mailbox_messages(&api, &v, messages.keys().copied()).await?;
            mailbox_added.append(&mut mailbox_from_events);

            block = rx.recv().await?;
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
            }
            program_ids.insert(pid);
        }
    }

    if !messages.is_empty() {
        tracing::error!("Unresolved messages: {messages:?}");
    }

    let unresolved_count = messages.len();
    let unresolved_sample: Vec<MessageId> = messages.keys().copied().take(10).collect();
    tracing::info!(
        start_block_target = ?block_hash,
        wait_for_event_blocks,
        batch_messages_total = initial_messages_len,
        results_total = results.len(),
        results_ok = ok_count,
        results_err = err_count,
        results_unknown = unknown_count,
        mailbox_added = mailbox_added.len(),
        program_ids = program_ids.len(),
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
        ..Default::default()
    })
}
