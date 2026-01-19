use alloy::primitives::Address;
use anyhow::Result;
use ethexe_ethereum::{
    Ethereum,
    abi::IMirror::{
        ExecutableBalanceTopUpRequested, MessageQueueingRequested, OwnedBalanceTopUpRequested,
        ReplyQueueingRequested, StateChanged, ValueClaimingRequested,
    },
};
use futures::{StreamExt, stream::FuturesUnordered};
use gear_call_gen::CallGenRng;
use gear_core::message::ReplyCode;
use gprimitives::{H256, MessageId};
use std::{collections::BTreeSet, marker::PhantomData};
use tokio::sync::broadcast::Receiver;
use tracing::instrument;

use crate::{
    args::{LoadParams, SeedVariant},
    batch::{
        context::Context,
        generator::{Batch, BatchGenerator, BatchWithSeed, RuntimeSettings},
        report::{BatchRunReport, Report},
    },
};

pub mod context;
pub mod generator;
pub mod report;

pub struct BatchPool<Rng: CallGenRng> {
    api: Ethereum,
    pool_size: usize,
    batch_size: usize,
    task_context: Context,
    rx: Receiver<Event>,
    _marker: PhantomData<Rng>,
}

/// Events emitted by mirror contract. Used to build mailbox and other context state for
/// batch report.
#[derive(Clone)]
pub struct Event {
    pub kind: EventKind,
    /// Address of the contract that emitted the event
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
    Message(ethexe_ethereum::abi::IMirror::Message),
    MessageCallFailed(ethexe_ethereum::abi::IMirror::MessageCallFailed),
    Reply(ethexe_ethereum::abi::IMirror::Reply),
    ReplyCallFailed(ethexe_ethereum::abi::IMirror::ReplyCallFailed),

    ValueClaimed(ethexe_ethereum::abi::IMirror::ValueClaimed),
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

impl<Rng: CallGenRng> BatchPool<Rng> {
    pub fn new(api: Ethereum, pool_size: usize, batch_size: usize, rx: Receiver<Event>) -> Self {
        Self {
            api,
            pool_size,
            batch_size,
            task_context: Context::new(),
            rx,
            _marker: PhantomData,
        }
    }

    pub async fn run(mut self, params: LoadParams, _rx: Receiver<Event>) -> Result<()> {
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
            batches.push(run_batch(api, batch_with_seed, self.rx.resubscribe()));
        }

        while let Some(report) = batches.next().await {
            self.process_run_report(report?);

            let api = Ethereum::from_provider(
                self.api.provider().clone(),
                self.api.router().address().into(),
            )
            .await?;
            let batch_with_seed = batch_gen.generate(self.task_context.clone());
            batches.push(run_batch(api, batch_with_seed, self.rx.resubscribe()));
        }

        unreachable!()
    }

    fn process_run_report(&mut self, report: BatchRunReport) {
        self.task_context.update(report.context_update);
    }
}

async fn run_batch(
    api: Ethereum,
    batch: BatchWithSeed,
    rx: Receiver<Event>,
) -> Result<BatchRunReport> {
    let (seed, batch) = batch.into();

    match run_batch_impl(api, batch, rx).await {
        Ok(report) => Ok(BatchRunReport::new(seed, report)),
        Err(err) => {
            tracing::info!("Batch failed: {err:?}");
            Ok(BatchRunReport::empty(seed))
        }
    }
}

#[instrument(skip_all)]
async fn run_batch_impl(api: Ethereum, batch: Batch, _rx: Receiver<Event>) -> Result<Report> {
    match batch {
        Batch::UploadProgram(args) => {
            let mut code_ids = Vec::with_capacity(args.len());

            for arg in args.iter() {
                let code_id = api
                    .router()
                    .request_code_validation_with_sidecar(&arg.0.0)
                    .await?
                    .code_id();
                code_ids.push(code_id);
            }

            for code_id in code_ids.iter().copied() {
                api.router().wait_code_validation(code_id).await?;
            }
            let mut program_ids = BTreeSet::new();
            for (arg, code_id) in args.iter().zip(code_ids.iter().copied()) {
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

                program_ids.insert(program.1);
            }
            Ok(Report {
                program_ids,
                ..Default::default()
            })
        }

        Batch::UploadCode(args) => {
            let mut code_ids = Vec::with_capacity(args.len());

            for arg in args.iter() {
                let code_id = api
                    .router()
                    .request_code_validation_with_sidecar(&arg.0)
                    .await?
                    .code_id();
                code_ids.push(code_id);
            }

            for code_id in code_ids.iter().copied() {
                api.router().wait_code_validation(code_id).await?;
            }

            Ok(Report {
                codes: code_ids.into_iter().collect(),
                ..Default::default()
            })
        }

        Batch::SendMessage(args) => {
            for arg in args.iter() {
                let to = arg.0.0;
                let mirror = api.mirror(ethexe_common::Address::try_from(to).unwrap());
                mirror.send_message(&arg.0.1, arg.0.3).await?;
            }

            Ok(Report {
                ..Default::default()
            })
        }

        Batch::ClaimValue(_args) => {
            tracing::warn!("ClaimValue batch is not implemented yet");
            Ok(Report {
                ..Default::default()
            })
        }

        Batch::SendReply(_args) => {
            tracing::warn!("SendReply batch is not implemented yet");
            Ok(Report {
                ..Default::default()
            })
        }

        Batch::CreateProgram(args) => {
            let mut programs = BTreeSet::new();
            for arg in args.iter() {
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
                programs.insert(program.1);
            }

            Ok(Report {
                program_ids: programs,
                ..Default::default()
            })
        }
    }
}
