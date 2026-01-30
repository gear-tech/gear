use alloy::{
    eips::BlockId,
    network::{Network, primitives::HeaderResponse},
    primitives::{Address, FixedBytes, LogData},
    providers::Provider,
    rpc::types::{Filter, Log},
    sol_types::SolEvent,
};
use anyhow::Result;
use ethexe_ethereum::{
    Ethereum,
    abi::IMirror::*,
    mirror::signatures::{
        EXECUTABLE_BALANCE_TOP_UP_REQUESTED, MESSAGE, MESSAGE_CALL_FAILED,
        MESSAGE_QUEUEING_REQUESTED, OWNED_BALANCE_TOP_UP_REQUESTED, REPLY, REPLY_CALL_FAILED,
        REPLY_QUEUEING_REQUESTED, STATE_CHANGED, VALUE_CLAIMED, VALUE_CLAIMING_REQUESTED,
    },
};
use ethexe_sdk::VaraEthApi;
use futures::{StreamExt, stream::FuturesUnordered};
use gear_call_gen::{CallGenRng, ClaimValueArgs, SendReplyArgs};
use gear_core::{
    ids::prelude::{CodeIdExt, MessageIdExt},
    message::ReplyCode,
};
use gprimitives::{ActorId, CodeId, H256, MessageId};
use std::{
    collections::{BTreeMap, BTreeSet},
    marker::PhantomData,
    sync::Arc,
    time::Duration,
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
    api: Ethereum,
    eth_rpc_url: String,
    pool_size: usize,
    batch_size: usize,
    task_context: Context,
    rx: Receiver<<alloy::network::Ethereum as Network>::HeaderResponse>,
    _marker: PhantomData<Rng>,
}

type MidMap = Arc<RwLock<BTreeMap<MessageId, ActorId>>>;

/// Events emitted by mirror contract. Used to build mailbox and other context state for
/// batch report.
#[derive(Debug, Clone)]
pub struct Event {
    pub kind: EventKind,
    /// Address of the contract that emitted the event
    #[allow(dead_code)]
    pub address: Address,
}

#[derive(Clone)]
pub enum EventKind {
    StateChanged(StateChanged),
    MessageQueueingRequested(MessageQueueingRequested),
    ReplyQueueingRequested(ReplyQueueingRequested),
    ValueClaimingRequested(ValueClaimingRequested),
    OwnedBalanceTopUpRequested(OwnedBalanceTopUpRequested),
    ExecutableBalanceTopUpRequested(ExecutableBalanceTopUpRequested),
    Message(Message),
    MessageCallFailed(MessageCallFailed),
    Reply(Reply),
    ReplyCallFailed(ReplyCallFailed),
    ValueClaimed(ValueClaimed),
}

impl std::fmt::Debug for EventKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::StateChanged(ev) => {
                write!(f, "StateChanged({})", H256(ev.stateHash.0))
            }

            Self::MessageQueueingRequested(ev) => {
                write!(
                    f,
                    "MessageQueueingRequested({}, {}, {}, {})",
                    MessageId::from(ev.id.0),
                    ev.payload,
                    ev.source,
                    ev.value
                )
            }

            Self::ReplyQueueingRequested(ev) => {
                write!(
                    f,
                    "ReplyQueueingRequested({}, {}, {}, {})",
                    MessageId::from(ev.repliedTo.0),
                    ev.source,
                    ev.payload,
                    ev.value
                )
            }

            Self::ValueClaimingRequested(ev) => {
                write!(
                    f,
                    "ValueClaimingRequested({}, {})",
                    H256::from(ev.claimedId.0),
                    ev.source
                )
            }

            Self::OwnedBalanceTopUpRequested(ev) => {
                write!(f, "OwnedBalanceTopUpRequested({})", ev.value)
            }

            Self::ExecutableBalanceTopUpRequested(ev) => {
                write!(f, "ExecutableBalanceTopUpRequested({})", ev.value)
            }

            Self::Message(message) => {
                write!(
                    f,
                    "Message({}, {}, {}, {})",
                    MessageId::from(message.id.0),
                    message.destination,
                    message.payload,
                    message.value
                )
            }

            Self::MessageCallFailed(message) => {
                write!(
                    f,
                    "MessageCallFailed({}, {}, {})",
                    MessageId::from(message.id.0),
                    message.destination,
                    message.value
                )
            }

            Self::Reply(reply) => {
                write!(
                    f,
                    "Reply({}, {}, {}, {})",
                    MessageId::from(reply.replyTo.0),
                    ReplyCode::from_bytes(reply.replyCode.0),
                    reply.payload,
                    reply.value
                )
            }
            Self::ReplyCallFailed(call) => {
                write!(
                    f,
                    "ReplyCallFailed({}, {}, {})",
                    MessageId::from(call.replyTo.0),
                    ReplyCode::from_bytes(call.replyCode.0),
                    call.value
                )
            }

            Self::ValueClaimed(ev) => {
                write!(
                    f,
                    "ValueClaimed({}, {})",
                    H256::from(ev.claimedId.0),
                    ev.value
                )
            }
        }
    }
}

impl Event {
    pub fn decode_rpc_log(log: Log<LogData>) -> Result<Option<Self>> {
        let kind = match log.topic0().copied() {
            Some(STATE_CHANGED) => {
                EventKind::StateChanged(StateChanged::decode_log_data(log.data())?)
            }
            Some(MESSAGE_QUEUEING_REQUESTED) => EventKind::MessageQueueingRequested(
                MessageQueueingRequested::decode_log_data(log.data())?,
            ),
            Some(REPLY_QUEUEING_REQUESTED) => EventKind::ReplyQueueingRequested(
                ReplyQueueingRequested::decode_log_data(log.data())?,
            ),
            Some(VALUE_CLAIMING_REQUESTED) => EventKind::ValueClaimingRequested(
                ValueClaimingRequested::decode_log_data(log.data())?,
            ),
            Some(OWNED_BALANCE_TOP_UP_REQUESTED) => EventKind::OwnedBalanceTopUpRequested(
                OwnedBalanceTopUpRequested::decode_log_data(log.data())?,
            ),
            Some(EXECUTABLE_BALANCE_TOP_UP_REQUESTED) => {
                EventKind::ExecutableBalanceTopUpRequested(
                    ExecutableBalanceTopUpRequested::decode_log_data(log.data())?,
                )
            }
            Some(MESSAGE) => EventKind::Message(Message::decode_log_data(log.data())?),
            Some(MESSAGE_CALL_FAILED) => {
                EventKind::MessageCallFailed(MessageCallFailed::decode_log_data(log.data())?)
            }
            Some(REPLY) => EventKind::Reply(Reply::decode_log_data(log.data())?),
            Some(REPLY_CALL_FAILED) => {
                EventKind::ReplyCallFailed(ReplyCallFailed::decode_log_data(log.data())?)
            }
            Some(VALUE_CLAIMED) => {
                EventKind::ValueClaimed(ValueClaimed::decode_log_data(log.data())?)
            }

            _ => return Ok(None),
        };

        Ok(Some(Event {
            kind,
            address: log.address(),
        }))
    }
}

impl<Rng: CallGenRng> BatchPool<Rng> {
    pub fn new(
        api: Ethereum,
        eth_rpc_url: String,
        pool_size: usize,
        batch_size: usize,
        rx: Receiver<<alloy::network::Ethereum as Network>::HeaderResponse>,
    ) -> Self {
        Self {
            api,
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

        while batches.len() != self.pool_size {
            let batch_with_seed = batch_gen.generate(self.task_context.clone());
            let api = Ethereum::from_provider(
                self.api.provider().clone(),
                self.api.router().address().into(),
            )
            .await?;
            let api1 = Ethereum::from_provider(
                self.api.provider().clone(),
                self.api.router().address().into(),
            )
            .await?;
            let vapi = VaraEthApi::new(&self.eth_rpc_url, api).await?;
            batches.push(run_batch(
                api1,
                vapi,
                batch_with_seed,
                self.rx.resubscribe(),
                mid_map.clone(),
            ));
        }

        while let Some(report) = batches.next().await {
            self.process_run_report(report?);

            let api = Ethereum::from_provider(
                self.api.provider().clone(),
                self.api.router().address().into(),
            )
            .await?;
            let api1 = Ethereum::from_provider(
                self.api.provider().clone(),
                self.api.router().address().into(),
            )
            .await?;
            let vapi = VaraEthApi::new(&self.eth_rpc_url, api).await?;
            let batch_with_seed = batch_gen.generate(self.task_context.clone());
            batches.push(run_batch(
                api1,
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

    match run_batch_impl(api, vapi, batch, rx, mid_map).await {
        Ok(report) => Ok(BatchRunReport::new(seed, report)),
        Err(err) => {
            tracing::info!("Batch failed: {err:?}");
            Ok(BatchRunReport::empty(seed))
        }
    }
}

#[instrument(skip_all)]
async fn run_batch_impl(
    api: Ethereum,
    vapi: VaraEthApi,
    batch: Batch,
    rx: Receiver<<alloy::network::Ethereum as Network>::HeaderResponse>,
    mid_map: MidMap,
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
                let salt_vec = if salt.len() != 32 {
                    let mut vec = Vec::with_capacity(32);
                    vec.extend_from_slice(&salt[..]);
                    while vec.len() < 32 {
                        vec.push(0);
                    }
                    vec
                } else {
                    salt.to_vec()
                };
                let program = api
                    .router()
                    .create_program(code_id, H256::from_slice(&salt_vec[..32]), None)
                    .await?;

                api.router()
                    .wvara()
                    .approve(program.1.to_address_lossy().0.into(), 500_000_000_000_000)
                    .await?;
                let mirror = api.mirror(program.1.to_address_lossy().0.into());
                mirror
                    .executable_balance_top_up(500_000_000_000_000)
                    .await?;
                tracing::debug!("[Call with id {call_id}]: Program created {}", program.1);
                match mirror.send_message(&arg.0.2, arg.0.4).await {
                    Ok((_, message_id)) => {
                        mid_map.write().await.insert(message_id, program.1);
                        messages.insert(message_id, (program.1, call_id));
                        tracing::debug!("[Call with id {call_id}]: Init message sent {message_id}",);
                    }
                    Err(err) => {
                        tracing::error!("Failed to send message: {err:?}");
                        return Err(err);
                    }
                }
                program_ids.insert(program.1);
            }

            process_events(api, messages, rx, block.hash()).await
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
                let message_id = if rand::random::<bool>() {
                    tracing::debug!("[Call with id {i}]: Sending injected message to {to}");
                    let mirror = vapi.mirror(to);
                    mirror.send_message_injected(&arg.0.1, arg.0.3).await?
                } else {
                    tracing::debug!(
                        "[Call with id {i}]: Sending message to {to} through Mirror contract"
                    );
                    let mirror = api.mirror(ethexe_common::Address::try_from(to).unwrap());
                    let (_, mid) = mirror.send_message(&arg.0.1, arg.0.3).await?;
                    mid
                };
                messages.insert(message_id, (to, i));
                mid_map.write().await.insert(message_id, to);
                tracing::debug!("[Call with id {i}]: Message sent #{message_id} to {to}");
            }

            process_events(api, messages, rx, block).await
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
                let mirror = api.mirror(ethexe_common::Address::try_from(actor_id).unwrap());
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
                let mirror = api.mirror(ethexe_common::Address::try_from(actor_id).unwrap());
                let _ = mirror.send_reply(mid, payload, value).await?;
                let reply_mid = MessageId::generate_reply(mid);
                mid_map.write().await.insert(reply_mid, actor_id);
                messages.insert(reply_mid, (actor_id, call_id));

                tracing::debug!(
                    "[Call with id: {call_id}]: Successfully replied to {mid} with value={value}"
                );
            }
            process_events(api, messages, rx, block)
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
                let salt_vec = if salt.len() != 32 {
                    let mut vec = Vec::with_capacity(32);
                    vec.extend_from_slice(salt);
                    while vec.len() < 32 {
                        vec.push(0);
                    }
                    vec
                } else {
                    salt.to_vec()
                };
                let program = api
                    .router()
                    .create_program(code_id, H256::from_slice(&salt_vec[0..32]), None)
                    .await?;
                api.router()
                    .wvara()
                    .approve(program.1.to_address_lossy().0.into(), 500_000_000_000_000)
                    .await?;
                let mirror = api.mirror(program.1.to_address_lossy().0.into());
                mirror
                    .executable_balance_top_up(500_000_000_000_000)
                    .await?;
                tracing::debug!("[Call with id: {call_id}]: Program created {}", program.1);
                // send init message to program with payload and value.
                match mirror.send_message(&arg.0.2, arg.0.4).await {
                    Ok((_, message)) => {
                        programs.insert(program.1);
                        messages.insert(message, (program.1, call_id));
                        tracing::debug!(
                            "[Call with id: {call_id}]: Successfully sent init message {message}"
                        );
                    }
                    Err(e) => {
                        tracing::error!(
                            "[Call with id: {call_id}]: Failed to send message to program: {e}",
                        );
                    }
                }
            }

            process_events(api, messages, rx, block_hash.hash()).await
        }
    }
}

/// Wait for the new events since provided `block_hash`.
async fn process_events(
    api: Ethereum,
    mut messages: BTreeMap<MessageId, (ActorId, usize)>,
    mut rx: Receiver<<alloy::network::Ethereum as Network>::HeaderResponse>,
    block_hash: FixedBytes<32>,
) -> Result<Report> {
    let wait_for_event_blocks = 30;
    let wait_for_events_millisec = 14 * 1000;
    let mut mailbox_added = BTreeSet::new();
    let results = {
        let mut block = rx.recv().await?;
        while block.hash() != block_hash {
            tokio::time::sleep(Duration::new(0, 500)).await;
            block =
                tokio::time::timeout(Duration::from_millis(wait_for_events_millisec), rx.recv())
                    .await
                    .map_err(|_| {
                        tracing::debug!("Timeout is reached while waiting for block events");
                        anyhow::anyhow!("Event waiting timed out")
                    })??;
        }

        let mut v = Vec::new();
        let mut current_bn = block.hash();
        let mut i = 0;
        while i < wait_for_event_blocks {
            if block.hash() != current_bn {
                current_bn = block.hash();
                i += 1;
            }

            let logs = api
                .provider()
                .get_logs(&Filter::new().at_block_hash(current_bn))
                .await?;

            for log in logs {
                if let Some(event) = Event::decode_rpc_log(log)? {
                    tracing::debug!("Relevant log discovered: {event:?}");
                    v.push(event);
                }
            }

            tokio::time::sleep(Duration::new(0, 100)).await;

            block =
                tokio::time::timeout(Duration::from_millis(wait_for_events_millisec), rx.recv())
                    .await
                    .map_err(|_| {
                        tracing::debug!("Timeout is reached while waiting for block events");
                        anyhow::anyhow!("Event waiting timed out")
                    })??;

            let mut mailbox_from_events = utils::capture_mailbox_messages(&api, &v)
                .await
                .expect("always valid");
            mailbox_added.append(&mut mailbox_from_events);
        }

        utils::err_waited_or_succeed_batch(&mut v, messages.keys().copied()).await
    };

    let mut program_ids = BTreeSet::new();

    for (mid, maybe_err) in results {
        if messages.is_empty() {
            break;
        }

        if let Some((pid, call_id)) = messages.remove(&mid) {
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
        unreachable!("unresolved messages")
    }

    tracing::debug!("Mailbox {:?}", mailbox_added);
    Ok(Report {
        program_ids,
        mailbox_data: mailbox_added.into(),
        ..Default::default()
    })
}
