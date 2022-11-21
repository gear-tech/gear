use crate::{
    args::{LoadParams, SeedVariant},
    utils::{self, LoaderRng, SwapResult},
};
use anyhow::{anyhow, Result};
use api::GearApiFacade;
use batch::{Batch, BatchWithSeed};
use context::Context;
use futures::{stream::FuturesUnordered, StreamExt};
use gclient::{Error as GClientError, EventProcessor, GearApi, Result as GClientResult};
use gear_core::ids::{MessageId, ProgramId};
use generators::{BatchGenerator, RuntimeSettings};
use primitive_types::H256;
pub use report::CrashAlert;
use report::{BatchRunReport, Report};
use std::{
    collections::{BTreeMap, BTreeSet},
    marker::PhantomData,
};
use tracing::instrument;

mod api;
mod batch;
mod context;
pub mod generators;
mod report;

type Seed = u64;
type CallId = usize;

pub struct BatchPool<Rng: LoaderRng> {
    api: GearApiFacade,
    pool_size: usize,
    batch_size: usize,
    tasks_context: Context,
    _phantom: PhantomData<Rng>,
}

impl<Rng: LoaderRng> BatchPool<Rng> {
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
        let inspect_crash_task = inspect_crash_events(api.into_gear_api());

        let run_result = tokio::select! {
            r = run_pool_task => r,
            // TODO spawn a task
            r = inspect_crash_task => r,
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

        let seed = loader_seed.unwrap_or_else(utils::now);
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
            let (ex_results, block_hash) = api.upload_program_batch(args).await?;
            let messages = process_ex_results(ex_results);
            process_events(api.into_gear_api(), messages, block_hash, true).await
        }
        Batch::UploadCode(args) => {
            let (ex_results, _) = api.upload_code_batch(args).await?;
            let ex_results = ex_results.into_iter().map(|r| r.map(|code| (code, ())));
            let codes = process_ex_results(ex_results);
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
            let (ex_results, block_hash) = api.send_message_batch(args).await?;
            let messages = process_ex_results(ex_results);
            process_events(api.into_gear_api(), messages, block_hash, false).await
        }
        Batch::CreateProgram(args) => {
            let (ex_results, block_hash) = api.create_program_batch(args).await?;
            let messages = process_ex_results(ex_results);
            process_events(api.into_gear_api(), messages, block_hash, true).await
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
    collect_programs: bool,
) -> Result<Report> {
    let now = utils::now();
    // States what amount of blocks we should wait for taking all the events about successful `messages` execution
    let wait_for_events_blocks = 10;
    // Multiply on five to be 100% sure if no events occurred, than node is crashed
    let wait_for_events_millisec = api.expected_block_time()? as usize * wait_for_events_blocks * 5;

    let results = loop {
        let r = match api.events_since(block_hash, wait_for_events_blocks).await {
            Ok(mut v) => v.err_or_succeed_batch(messages.keys().copied()).await,
            Err(e) => Err(e),
        };

        if (utils::now() - now) as usize > wait_for_events_millisec {
            tracing::debug!("Timeout is reached while waiting for events");
            return Err(anyhow!(utils::EVENTS_TIMEOUT_ERR_STR));
        }

        if matches!(r, Err(GClientError::EventNotFoundInIterator)) {
            continue;
        } else {
            break r;
        }
    };

    let mut program_ids = collect_programs.then(BTreeSet::new);

    for (mid, maybe_err) in results? {
        let (pid, call_id) = messages.remove(&mid).expect("Infallible");

        if let Some(expl) = maybe_err {
            tracing::debug!("[Call with id: {call_id}]: {mid:#.2} executing within program '{pid:#.2}' ended with a trap: '{expl}'");
        } else {
            tracing::debug!("[Call with id: {call_id}]: {mid:#.2} successfully executed within program '{pid:#.2}'");
            program_ids.as_mut().map(|ids| ids.insert(pid));
        }
    }

    Ok(Report {
        program_ids: program_ids.unwrap_or_default(),
        ..Default::default()
    })
}

async fn inspect_crash_events(api: GearApi) -> Result<()> {
    let mut event_listener = api.subscribe().await?;
    // Error means either event is not found an can't be found
    // in the listener, or some other error during event
    // parsing occurred.
    let crash_block_hash = event_listener.queue_processing_reverted().await?;

    let crash_err = CrashAlert::MsgProcessingStopped;
    tracing::info!("{crash_err} at block hash {crash_block_hash:?}");

    Err(crash_err.into())
}
