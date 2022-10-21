use crate::{
    args::SeedVariant,
    utils::{self, GearApiProducer, LoaderRng},
};
use anyhow::Result;
use batch::{Batch, BatchWithSeed};
use context::Context;
use futures::{stream::FuturesUnordered, StreamExt};
use gclient::{Error as GClientError, EventProcessor, GearApi, Result as GClientResult};
use gear_core::ids::{MessageId, ProgramId};
use generators::BatchGenerator;
use primitive_types::H256;
pub use report::CrashAlert;
use report::{BatchRunReport, Report};
use std::{
    collections::{BTreeMap, BTreeSet},
    marker::PhantomData,
};
use tracing::instrument;

mod batch;
mod context;
pub mod generators;
mod report;

type Seed = u64;
type CallId = usize;

pub struct BatchPool<Rng: LoaderRng> {
    api_producer: GearApiProducer,
    pool_size: usize,
    batch_size: usize,
    tasks_context: Context,
    _phantom: PhantomData<Rng>,
}

impl<Rng: LoaderRng> BatchPool<Rng> {
    pub fn new(api_producer: GearApiProducer, pool_size: usize, batch_size: usize) -> Self {
        Self {
            api_producer,
            pool_size,
            batch_size,
            tasks_context: Context::new(),
            _phantom: PhantomData,
        }
    }

    #[instrument(skip_all)]
    pub async fn run(
        &mut self,
        code_seed_type: Option<SeedVariant>,
        node_stopper: String,
    ) -> Result<()> {
        let api = self.api_producer.current();

        let run_pool_task = self.run_pool_loop(code_seed_type);
        let inspect_crash_task = inspect_crash_events(api, node_stopper);

        tokio::select! {
            r = run_pool_task => r,
            r = inspect_crash_task => r,
        }
    }

    #[instrument(skip_all)]
    async fn run_pool_loop(&mut self, code_seed_type: Option<SeedVariant>) -> Result<()> {
        let mut batches = FuturesUnordered::new();

        let seed = utils::now();
        tracing::info!(
            message = "Running task pool with params",
            seed,
            pool_size = self.pool_size,
            batch_size = self.batch_size
        );

        let mut batch_gen = BatchGenerator::<Rng>::new(seed, self.batch_size, code_seed_type);

        while batches.len() != self.pool_size {
            let batch_with_seed = batch_gen.generate(self.tasks_context.clone());
            let api = self.api_producer.produce();

            batches.push(run_batch(api, batch_with_seed));
        }

        loop {
            if let Some(report_res) = batches.next().await {
                self.process_run_report(report_res?).await;

                let batch_with_seed = batch_gen.generate(self.tasks_context.clone());
                let api = self.api_producer.produce();

                batches.push(run_batch(api, batch_with_seed));
            }
        }
    }

    async fn process_run_report(&mut self, report: BatchRunReport) {
        let BatchRunReport {
            context_update,
            id: seed,
            err,
            ..
        } = report;

        if let Some(err) = err {
            tracing::debug!("Error occurred while running batch[{seed}]: {err}");
        }

        self.tasks_context.update(context_update);
    }
}

#[instrument(skip_all, fields(seed = batch.seed, batch_type = batch.batch_str()))]
async fn run_batch(api: GearApi, batch: BatchWithSeed) -> Result<BatchRunReport> {
    let (seed, batch) = batch.into();
    match run_batch_impl(api, batch).await {
        Ok(report) => Ok(BatchRunReport::new(seed, report)),
        Err(err) => {
            if err.is::<CrashAlert>() {
                // Report crash error
                tracing::info!("{err}");

                Err(err)
            } else {
                Ok(BatchRunReport::from_err(seed, err))
            }
        }
    }
}

#[instrument(skip_all)]
async fn run_batch_impl(api: GearApi, batch: Batch) -> Result<Report> {
    // Order of the results of each extrinsic execution in the batch
    // is the same as in the input set of calls in the batch.
    // See: https://paritytech.github.io/substrate/master/src/pallet_utility/lib.rs.html#452-468
    match batch {
        Batch::UploadProgram(args) => {
            if let Ok(r) =
                utils::with_timeout(api.upload_program_bytes_batch(utils::convert_iter(args))).await
            {
                let (ex_results, block_hash) = r.map_err(utils::try_node_dead_err)?;
                let messages = process_ex_results(ex_results);
                return process_events(api, messages, block_hash, true).await;
            }
        }
        Batch::UploadCode(args) => {
            if let Ok(r) =
                utils::with_timeout(api.upload_code_batch(utils::convert_iter::<Vec<_>, _>(args)))
                    .await
            {
                let ex_results = r
                    .map_err(utils::try_node_dead_err)?
                    .0
                    .into_iter()
                    .map(|r| r.map(|code| (code, ())));
                let codes = process_ex_results(ex_results);
                for (code_id, (_, call_id)) in codes.iter() {
                    tracing::debug!(
                        "[Call with id: {call_id}]: Successfully deployed code with id '{code_id}'"
                    );
                }

                return Ok(Report {
                    codes: codes.keys().copied().collect(),
                    ..Default::default()
                });
            }
        }
        Batch::SendMessage(args) => {
            if let Ok(r) =
                utils::with_timeout(api.send_message_bytes_batch(utils::convert_iter(args))).await
            {
                let (ex_results, block_hash) = r.map_err(utils::try_node_dead_err)?;
                let messages = process_ex_results(ex_results);
                return process_events(api, messages, block_hash, false).await;
            }
        }
        Batch::CreateProgram(args) => {
            if let Ok(r) =
                utils::with_timeout(api.create_program_bytes_batch(utils::convert_iter(args))).await
            {
                let (ex_results, block_hash) = r.map_err(utils::try_node_dead_err)?;
                let messages = process_ex_results(ex_results);
                return process_events(api, messages, block_hash, false).await;
            }
        }
    }

    Err(CrashAlert::Timeout.into())
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
    let results: GClientResult<Vec<(MessageId, Option<String>)>>;
    let now = utils::now();
    // States what amount of blocks we should wait for taking all the events about successful `messages` execution
    let wait_for_events_blocks = 10;

    loop {
        let r = match api.events_since(block_hash, wait_for_events_blocks).await {
            Ok(mut v) => v.err_or_succeed_batch(messages.keys().copied()).await,
            Err(e) => Err(e),
        };

        // If one block is considered to be produced in 1 second, than we wait for 10000 millis (1 sec) * `wait_for_events_blocks`
        // We also multiply it on the 5 just to be 100% sure if no events occurred, than node is crashed
        if (utils::now() - now) as usize > wait_for_events_blocks * 1000 * 5 {
            tracing::debug!("Timeout is reached while waiting for events");
            return Err(CrashAlert::Timeout.into());
        }

        if matches!(r, Err(GClientError::EventNotFoundInIterator)) {
            continue;
        } else {
            results = r;
            break;
        }
    }

    let mut program_ids = if collect_programs {
        Some(BTreeSet::new())
    } else {
        None
    };

    for (mid, maybe_err) in results.map_err(utils::try_node_dead_err)? {
        let (pid, call_id) = messages.remove(&mid).expect("Infallible");

        if let Some(expl) = maybe_err {
            tracing::debug!("[Call with id: {call_id}]: {mid:#.2} executing within program '{pid:#.2}' ended with a trap: '{expl}'");
        } else {
            tracing::debug!("[Call with id: {call_id}]: {mid:#.2} successfully executed within program '{pid:#.2}'");
            program_ids = program_ids.map(|mut ids| {
                ids.insert(pid);
                ids
            });
        }
    }

    Ok(Report {
        program_ids: program_ids.unwrap_or_default(),
        ..Default::default()
    })
}

async fn inspect_crash_events(api: GearApi, node_stopper: String) -> Result<()> {
    let mut event_listener = api.subscribe().await?;
    let res = event_listener
        .queue_processing_reverted()
        .await
        .map_err(|e| e.into());
    if res.is_ok() {
        let crash_err = CrashAlert::MsgProcessingStopped;
        tracing::info!("{crash_err}");

        utils::stop_node(node_stopper).await?;

        Err(crash_err.into())
    } else {
        res
    }
}
