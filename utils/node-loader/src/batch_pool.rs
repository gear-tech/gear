use crate::{
    args::{LoadParams, SeedVariant},
    utils::{self, SwapResult},
};
use anyhow::{Result, anyhow};
use api::GearApiFacade;
use context::Context;
use futures::{Future, StreamExt, stream::FuturesUnordered};
use gclient::{GearApi, Result as GClientResult};
use gear_call_gen::{CallGenRng, ClaimValueArgs, SendReplyArgs};
use gear_core::ids::{ActorId, MessageId};
use generators::{Batch, BatchGenerator, BatchWithSeed, RuntimeSettings};
use gsdk::{
    AsGear,
    ext::subxt::utils::H256,
    gear::{Event, gear::Event as GearEvent},
};
pub use report::CrashAlert;
use report::{BatchRunReport, MailboxReport, Report};
use std::{
    collections::{BTreeMap, BTreeSet},
    marker::PhantomData,
};
use tokio::{sync::broadcast::Receiver, time::Duration};
use tracing::instrument;

pub mod api;
mod context;
pub mod generators;
mod report;

type Seed = u64;
type CallId = usize;
type EventsReceiver = Receiver<gsdk::subscription::BlockEvents>;

pub struct BatchPool<Rng: CallGenRng> {
    api: GearApiFacade,
    pool_size: usize,
    batch_size: usize,
    tasks_context: Context,
    rx: EventsReceiver,
    _phantom: PhantomData<Rng>,
}

impl<Rng: CallGenRng> BatchPool<Rng> {
    pub fn new(
        api: GearApiFacade,
        batch_size: usize,
        pool_size: usize,
        rx: EventsReceiver,
    ) -> Self {
        Self {
            api,
            pool_size,
            batch_size,
            tasks_context: Context::new(),
            rx,
            _phantom: PhantomData,
        }
    }

    /// Consume `BatchPool` and spawn tasks.
    ///
    /// - `run_pool_task` - the main task for sending and processing batches.
    /// - `inspect_crash_task` - background task monitors when message processing stops.
    /// - `renew_balance_task` - periodically setting a new balance for the user account.
    ///
    /// Wait for any task to return result with `tokio::select!`.
    pub async fn run(mut self, params: LoadParams, rx: EventsReceiver) -> Result<()> {
        let gear_api = self.api.clone().into_gear_api();
        let run_pool_task = self.run_pool_loop(params.loader_seed, params.code_seed_type);
        let inspect_crash_task = tokio::spawn(inspect_crash_events(rx));
        let renew_balance_task =
            tokio::spawn(create_renew_balance_task(gear_api, params.root).await?);

        let run_result = tokio::select! {
            r = run_pool_task => r,
            r = inspect_crash_task => r?,
            r = renew_balance_task => r?,
        };

        if let Err(ref e) = run_result {
            tracing::info!("Pool run ends up with an error: {e:?}");
            utils::stop_node(params.node_stopper).await?;
        }

        run_result
    }

    /// The main `BatchPool` logic.
    ///
    /// Creates a new `BatchGenerator` with the provided `loader_seed`.
    #[instrument(skip_all)]
    async fn run_pool_loop(
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

        let rt_settings = RuntimeSettings::new(&self.api).await?;
        let mut batch_gen =
            BatchGenerator::<Rng>::new(seed, self.batch_size, code_seed_type, rt_settings);

        while batches.len() != self.pool_size {
            let api = self.api.clone();
            let batch_with_seed = batch_gen.generate(self.tasks_context.clone());

            batches.push(run_batch(api, batch_with_seed, self.rx.resubscribe()));
        }

        while let Some(report_res) = batches.next().await {
            self.process_run_report(report_res?).await;

            let api = self.api.clone();
            let batch_with_seed = batch_gen.generate(self.tasks_context.clone());

            batches.push(run_batch(api, batch_with_seed, self.rx.resubscribe()));
        }

        unreachable!()
    }

    async fn process_run_report(&mut self, report: BatchRunReport) {
        self.tasks_context.update(report.context_update);
    }
}

/// Runs the generated `BatchWithSeed` with the provided `GearApiFacade` and `EventsReceiver` to handle produced events.
#[instrument(skip_all, fields(seed = batch.seed, batch_type = batch.batch_str()))]
async fn run_batch(
    api: GearApiFacade,
    batch: BatchWithSeed,
    rx: EventsReceiver,
) -> Result<BatchRunReport> {
    let (seed, batch) = batch.into();
    match run_batch_impl(api, batch, rx).await {
        Ok(report) => Ok(BatchRunReport::new(seed, report)),
        Err(err) => {
            // Propagate crash error or return report
            CrashAlert::try_from(err)
                .inspect(|crash_err| {
                    tracing::info!("{crash_err}");
                })
                .map_err(|err| tracing::debug!("Error occurred while running batch: {err}"))
                .swap_result()?;

            Ok(BatchRunReport::empty(seed))
        }
    }
}

#[instrument(skip_all)]
async fn run_batch_impl(
    mut api: GearApiFacade,
    batch: Batch,
    rx: EventsReceiver,
) -> Result<Report> {
    // Order of the results of each extrinsic execution in the batch
    // is the same as in the input set of calls in the batch.
    // See: https://paritytech.github.io/substrate/master/src/pallet_utility/lib.rs.html#452-468
    match batch {
        Batch::UploadProgram(args) => {
            let (extrinsic_results, block_hash) = api.upload_program_batch(args).await?;
            let messages = process_ex_results(extrinsic_results);
            process_events(api.into_gear_api(), messages, block_hash, rx).await
        }
        Batch::UploadCode(args) => {
            let (extrinsic_results, _) = api.upload_code_batch(args).await?;
            let extrinsic_results = extrinsic_results
                .into_iter()
                .map(|r| r.map(|code| (code, ())));
            let codes = process_ex_results(extrinsic_results);
            for (code_id, (_, call_id)) in codes.iter() {
                tracing::debug!(
                    "[Call with id: {call_id}]: Successfully deployed code with id '{code_id}'"
                );
            }

            Ok(Report {
                codes: codes.keys().copied().collect(),
                ..Default::default()
            })
        }
        Batch::SendMessage(args) => {
            let (extrinsic_results, block_hash) = api.send_message_batch(args).await?;
            let messages = process_ex_results(extrinsic_results);
            process_events(api.into_gear_api(), messages, block_hash, rx).await
        }
        Batch::CreateProgram(args) => {
            let (extrinsic_results, block_hash) = api.create_program_batch(args).await?;
            let messages = process_ex_results(extrinsic_results);
            process_events(api.into_gear_api(), messages, block_hash, rx).await
        }
        Batch::SendReply(args) => {
            let removed_from_mailbox = args.clone().into_iter().map(|SendReplyArgs((mid, ..))| mid);

            let (extrinsic_results, block_hash) = api.send_reply_batch(args).await?;
            let messages = process_ex_results(extrinsic_results);
            process_events(api.into_gear_api(), messages, block_hash, rx)
                .await
                .map(|mut report| {
                    report.mailbox_data.append_removed(removed_from_mailbox);
                    report
                })
        }
        Batch::ClaimValue(args) => {
            let removed_from_mailbox = args.clone().into_iter().map(|ClaimValueArgs(mid)| mid);

            let (extrinsic_results, _) = api.claim_value_batch(args).await?;
            let extrinsic_results = extrinsic_results
                .into_iter()
                .zip(removed_from_mailbox.clone())
                .map(|(r, mid)| r.map(|value| (mid, value)));
            for (mid, (value, call_id)) in process_ex_results(extrinsic_results) {
                tracing::debug!(
                    "[Call with id: {call_id}]: Successfully claimed {value} amount from message {mid}."
                );
            }

            Ok(Report {
                mailbox_data: MailboxReport {
                    removed: BTreeSet::from_iter(removed_from_mailbox),
                    ..Default::default()
                },
                ..Default::default()
            })
        }
    }
}

fn process_ex_results<Key: Ord, Value>(
    ex_results: impl IntoIterator<Item = GClientResult<(Key, Value)>>,
) -> BTreeMap<Key, (Value, CallId)> {
    let mut res = BTreeMap::<Key, (Value, CallId)>::new();

    for (i, r) in ex_results.into_iter().enumerate() {
        let call_id = i + 1;
        match r {
            Ok((key, value)) => {
                res.insert(key, (value, call_id));
                tracing::debug!("[Call with id: {call_id}]: Successfully executed.")
            }
            Err(e) => tracing::debug!("[Call with id: {call_id}]: Failed: '{e:?}'"),
        }
    }

    res
}

/// Waiting for the new events since provided `block_hash`.
async fn process_events(
    api: GearApi,
    mut messages: BTreeMap<MessageId, (ActorId, usize)>,
    block_hash: H256,
    mut rx: EventsReceiver,
) -> Result<Report> {
    // States what amount of blocks we should wait for taking all the events about successful `messages` execution
    let wait_for_events_blocks = 30;
    // Multiply on five to be 100% sure if no events occurred, then node is crashed
    let wait_for_events_millisec = api.expected_block_time()? * wait_for_events_blocks * 5;

    let mut mailbox_added = BTreeSet::new();

    let results = {
        let mut events = rx.recv().await?;

        // Wait with a timeout until the `EventsReceiver` receives the expected block hash.
        while events.block_hash() != block_hash {
            tokio::time::sleep(Duration::new(0, 500)).await;
            events =
                tokio::time::timeout(Duration::from_millis(wait_for_events_millisec), rx.recv())
                    .await
                    .map_err(|_| {
                        tracing::debug!("Timeout is reached while waiting for events");
                        anyhow!(utils::EVENTS_TIMEOUT_ERR_STR)
                    })??;
        }

        // Wait for next n blocks and push new events.
        let mut v = Vec::new();
        let mut current_bh = events.block_hash();
        let mut i = 0;
        while i < wait_for_events_blocks {
            if events.block_hash() != current_bh {
                current_bh = events.block_hash();
                i += 1;
            }
            for event in events.iter() {
                let event = event?.as_gear()?;
                v.push(event);
            }
            tokio::time::sleep(Duration::new(0, 100)).await;
            events =
                tokio::time::timeout(Duration::from_millis(wait_for_events_millisec), rx.recv())
                    .await
                    .map_err(|_| {
                        tracing::debug!("Timeout is reached while waiting for events");
                        anyhow!(utils::EVENTS_TIMEOUT_ERR_STR)
                    })??;
        }

        let mut mailbox_from_events = utils::capture_mailbox_messages(&api, &v)
            .await
            .expect("always valid by definition");
        mailbox_added.append(&mut mailbox_from_events);

        utils::err_waited_or_succeed_batch(&mut v, messages.keys().copied())
    };

    let mut program_ids = BTreeSet::new();

    for (mid, maybe_err) in results {
        // We receive here a lot of different events that may have no shared context
        // with current messages we are expecting so making one-to-one relations
        // is wrong. But we are expecting that all messages are done
        // (removed from expectation map).
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
                    "[Call with id: {call_id}]: {mid:#.2} successfully executed within program '{pid:#.2}'"
                );
                program_ids.insert(pid);
            }
        }
    }

    if !messages.is_empty() {
        unreachable!("Unresolved messages")
    }

    Ok(Report {
        program_ids,
        mailbox_data: mailbox_added.into(),
        ..Default::default()
    })
}

async fn inspect_crash_events(mut rx: EventsReceiver) -> Result<()> {
    // Error means either event is not found and can't be found
    // in the listener, or some other error during event
    // parsing occurred.
    while let Ok(events) = tokio::time::timeout(Duration::from_secs(90), rx.recv()).await? {
        let bh = events.block_hash();
        for event in events.iter() {
            let event = event?.as_gear()?;
            if matches!(event, Event::Gear(GearEvent::QueueNotProcessed)) {
                let crash_err = CrashAlert::MsgProcessingStopped;
                tracing::info!("{crash_err} at block hash {bh:?}");
                return Err(crash_err.into());
            }
        }
    }

    Ok(())
}

async fn create_renew_balance_task(
    user_api: GearApi,
    root: String,
) -> Result<impl Future<Output = Result<()>>> {
    let user_address = user_api.account_id().clone();
    let user_target_balance = user_api.free_balance(&user_address).await?;

    let root_api = user_api.with(root)?;
    let root_address = root_api.account_id().clone();
    let root_target_balance = root_api.free_balance(&root_address).await?;

    // every 100 blocks renew balance
    let duration_millis = root_api.expected_block_time()? * 100;

    tracing::info!(
        "Renewing balances every {} seconds, user target balance is {}, authority target balance is {}",
        duration_millis / 1000,
        user_target_balance,
        root_target_balance
    );

    // Every `duration_millis` milliseconds updates authority and user (batch sender) balances
    // to target values.
    Ok(async move {
        loop {
            tokio::time::sleep(Duration::from_millis(duration_millis)).await;

            let user_balance_demand = {
                let current = root_api.free_balance(&user_address).await?;
                user_target_balance.saturating_sub(current)
            };
            tracing::debug!("User balance demand {user_balance_demand}");

            // Calling `force_set_balance` for `user` is potentially dangerous, because getting actual
            // reserved balance is a complicated task, as reserved balance is changed by another
            // task, which loads the node.
            //
            // Reserved balance mustn't be changed as it can cause runtime panics within reserving
            // or unreserving funds logic.
            root_api
                .force_set_balance(
                    root_address.clone(),
                    root_target_balance + user_balance_demand,
                )
                .await
                .map_err(|e| {
                    tracing::debug!("Failed to set balance of the root address: {e}");
                    e
                })?;
            root_api
                .transfer_keep_alive(
                    ActorId::new(user_address.clone().into()),
                    user_balance_demand,
                )
                .await
                .map_err(|e| {
                    tracing::debug!("Failed to transfer to user address: {e}");
                    e
                })?;

            tracing::debug!("Successfully renewed balances!");
        }
    })
}
