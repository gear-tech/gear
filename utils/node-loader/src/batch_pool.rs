use crate::{
    args::{LoadParams, SeedVariant},
    utils::{self, SwapResult},
};
use anyhow::{anyhow, Result};
use api::GearApiFacade;
use context::Context;
use futures::{stream::FuturesUnordered, Future, StreamExt};
use gclient::{Error as GClientError, EventProcessor, GearApi, Result as GClientResult};
use gear_call_gen::{CallGenRng, ClaimValueArgs, SendReplyArgs};
use gear_core::ids::{MessageId, ProgramId};
use generators::{Batch, BatchGenerator, BatchWithSeed, RuntimeSettings};
use primitive_types::H256;
pub use report::CrashAlert;
use report::{BatchRunReport, Report};
use std::{
    collections::{BTreeMap, BTreeSet},
    marker::PhantomData,
    time::Duration,
};
use tracing::instrument;

use self::report::MailboxReport;

mod api;
mod context;
pub mod generators;
mod report;

type Seed = u64;
type CallId = usize;

pub struct BatchPool<Rng: CallGenRng> {
    api: GearApiFacade,
    pool_size: usize,
    batch_size: usize,
    tasks_context: Context,
    _phantom: PhantomData<Rng>,
}

impl<Rng: CallGenRng> BatchPool<Rng> {
    fn new(api: GearApiFacade, batch_size: usize, pool_size: usize) -> Self {
        Self {
            api,
            pool_size,
            batch_size,
            tasks_context: Context::new(),
            _phantom: PhantomData,
        }
    }

    pub async fn run(params: LoadParams) -> Result<()> {
        let api = GearApiFacade::try_new(params.node, params.user).await?;
        let mut batch_pool = Self::new(api.clone(), params.batch_size, params.workers);

        let run_pool_task = batch_pool.run_pool_loop(params.loader_seed, params.code_seed_type);
        let inspect_crash_task = inspect_crash_events(api.clone().into_gear_api());
        let renew_balance_task =
            create_renew_balance_task(api.into_gear_api(), params.root).await?;

        // TODO 1876 separately spawned tasks
        let run_result = tokio::select! {
            r = run_pool_task => r,
            r = inspect_crash_task => r,
            r = renew_balance_task => r,
        };

        if let Err(ref e) = run_result {
            tracing::info!("Pool run ends up with an error: {e:?}");
            utils::stop_node(params.node_stopper).await?;
        }

        run_result
    }

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

            batches.push(run_batch(api, batch_with_seed));
        }

        while let Some(report_res) = batches.next().await {
            self.process_run_report(report_res?).await;

            let api = self.api.clone();
            let batch_with_seed = batch_gen.generate(self.tasks_context.clone());

            batches.push(run_batch(api, batch_with_seed));
        }

        unreachable!()
    }

    async fn process_run_report(&mut self, report: BatchRunReport) {
        self.tasks_context.update(report.context_update);
    }
}

#[instrument(skip_all, fields(seed = batch.seed, batch_type = batch.batch_str()))]
async fn run_batch(api: GearApiFacade, batch: BatchWithSeed) -> Result<BatchRunReport> {
    let (seed, batch) = batch.into();
    match run_batch_impl(api, batch).await {
        Ok(report) => Ok(BatchRunReport::new(seed, report)),
        Err(err) => {
            // Propagate crash error or return report
            CrashAlert::try_from(err)
                .map(|crash_err| {
                    tracing::info!("{crash_err}");
                    crash_err
                })
                .map_err(|err| tracing::debug!("Error occurred while running batch: {err}"))
                .swap_result()?;

            Ok(BatchRunReport::empty(seed))
        }
    }
}

#[instrument(skip_all)]
async fn run_batch_impl(mut api: GearApiFacade, batch: Batch) -> Result<Report> {
    // Order of the results of each extrinsic execution in the batch
    // is the same as in the input set of calls in the batch.
    // See: https://paritytech.github.io/substrate/master/src/pallet_utility/lib.rs.html#452-468
    match batch {
        Batch::UploadProgram(args) => {
            let (extrinsic_results, block_hash) = api.upload_program_batch(args).await?;
            let messages = process_ex_results(extrinsic_results);
            process_events(api.into_gear_api(), messages, block_hash).await
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
            process_events(api.into_gear_api(), messages, block_hash).await
        }
        Batch::CreateProgram(args) => {
            let (extrinsic_results, block_hash) = api.create_program_batch(args).await?;
            let messages = process_ex_results(extrinsic_results);
            process_events(api.into_gear_api(), messages, block_hash).await
        }
        Batch::SendReply(args) => {
            let removed_from_mailbox = args.clone().into_iter().map(|SendReplyArgs((mid, ..))| mid);

            let (extrinsic_results, block_hash) = api.send_reply_batch(args).await?;
            let messages = process_ex_results(extrinsic_results);
            process_events(api.into_gear_api(), messages, block_hash)
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

async fn process_events(
    api: GearApi,
    mut messages: BTreeMap<MessageId, (ProgramId, usize)>,
    block_hash: H256,
) -> Result<Report> {
    let now = gear_utils::now_millis();
    // States what amount of blocks we should wait for taking all the events about successful `messages` execution
    let wait_for_events_blocks = 10;
    // Multiply on five to be 100% sure if no events occurred, then node is crashed
    let wait_for_events_millisec = api.expected_block_time()? as usize * wait_for_events_blocks * 5;

    let mut mailbox_added = BTreeSet::new();

    let results = loop {
        let r = match api.events_since(block_hash, wait_for_events_blocks).await {
            Ok(mut v) => {
                // `gclient::EventProcessor` implementation on `IntoIterator` implementor clones `self` without mutating
                // it, although `proc` and `proc_many` take mutable reference on `self`.
                let mut mailbox_from_events = utils::capture_mailbox_messages(&api, &mut v)
                    .await
                    .expect("always valid by definition");
                mailbox_added.append(&mut mailbox_from_events);

                v.err_or_succeed_batch(messages.keys().copied()).await
            }
            Err(e) => Err(e),
        };

        if (gear_utils::now_millis() - now) as usize > wait_for_events_millisec {
            tracing::debug!("Timeout is reached while waiting for events");
            return Err(anyhow!(utils::EVENTS_TIMEOUT_ERR_STR));
        }

        if matches!(r, Err(GClientError::EventNotFoundInIterator)) {
            continue;
        } else {
            break r;
        }
    };

    let mut program_ids = BTreeSet::new();

    for (mid, maybe_err) in results? {
        let (pid, call_id) = messages.remove(&mid).expect("Infallible");

        if let Some(expl) = maybe_err {
            tracing::debug!("[Call with id: {call_id}]: {mid:#.2} executing within program '{pid:#.2}' ended with a trap: '{expl}'");
        } else {
            tracing::debug!("[Call with id: {call_id}]: {mid:#.2} successfully executed within program '{pid:#.2}'");
            program_ids.insert(pid);
        }
    }

    Ok(Report {
        program_ids,
        mailbox_data: mailbox_added.into(),
        ..Default::default()
    })
}

async fn inspect_crash_events(api: GearApi) -> Result<()> {
    let mut event_listener = api.subscribe().await?;
    // Error means either event is not found and can't be found
    // in the listener, or some other error during event
    // parsing occurred.
    let crash_block_hash = event_listener.queue_processing_reverted().await?;

    let crash_err = CrashAlert::MsgProcessingStopped;
    tracing::info!("{crash_err} at block hash {crash_block_hash:?}");

    Err(crash_err.into())
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

            // Calling `set_balance` for `user` is potentially dangerous, because getting actual
            // reserved balance is a complicated task, as reserved balance is changed by another
            // task, which loads the node.
            //
            // Reserved balance mustn't be changed as it can cause runtime panics within reserving
            // or unreserving funds logic.
            root_api
                .set_balance(
                    root_address.clone(),
                    root_target_balance + user_balance_demand,
                    0,
                )
                .await
                .map_err(|e| {
                    tracing::debug!("Failed to set balance of the root address: {e}");
                    e
                })?;
            root_api
                .transfer(ProgramId::from(user_address.as_ref()), user_balance_demand)
                .await
                .map_err(|e| {
                    tracing::debug!("Failed to transfer to user address: {e}");
                    e
                })?;

            tracing::debug!("Successfully renewed balances!");
        }
    })
}
