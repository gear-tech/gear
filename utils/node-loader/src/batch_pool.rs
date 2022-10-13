use crate::{
    args::SeedVariant,
    utils::{self, GearApiProducer, LoaderRng},
};
use anyhow::anyhow;
use batch::Batch;
use context::Context;
use futures::{stream::FuturesUnordered, StreamExt};
use gclient::{Error, EventProcessor, GearApi, Result};
use gear_core::ids::MessageId;
use generators::BatchGenerator;
use report::BatchRunReport;
use std::{
    collections::{BTreeMap, BTreeSet},
    marker::PhantomData,
};
use tracing::instrument;

use self::{batch::BatchWithSeed, report::Report};

mod batch;
mod context;
pub mod generators;
mod report;

type Seed = u64;

/* TODO
1. Fix grishasobol's case and case discussed with breathx
*/

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
    pub async fn run(&mut self, code_seed_type: Option<SeedVariant>) {
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

        while let Some(report) = batches.next().await {
            self.process_run_report(report);
            let batch_with_seed = batch_gen.generate(self.tasks_context.clone());
            let api = self.api_producer.produce();

            batches.push(run_batch(api, batch_with_seed));
        }
    }

    fn process_run_report(&mut self, report: BatchRunReport) {
        let BatchRunReport {
            context_update,
            blocks_stopped,
            id: seed,
            err,
        } = report;

        if let Some(err) = err {
            tracing::debug!("Error occurred {err:?}");
        }

        if blocks_stopped {
            // TODO should trigger remote process, which takes snapshot of the node
            tracing::info!("Blocks production has stopped while executing messages of the batch with id: {seed}. \
            Possibly, node panicked. Stopping loader");
            panic!("Ending loader.")
        }

        self.tasks_context.update(context_update);
    }
}

#[instrument(skip_all, fields(seed = batch.seed, batch_type = batch.batch_str()))]
async fn run_batch(api: GearApi, batch: BatchWithSeed) -> BatchRunReport {
    let (seed, batch) = batch.into();
    match run_batch_impl(api, batch).await {
        Ok(report) => BatchRunReport::new(seed, report),
        Err(err) => BatchRunReport::from_err(err.into()),
    }
}

#[instrument(skip_all)]
// TODO Simplify when the tool is stabilized
async fn run_batch_impl(api: GearApi, batch: Batch) -> Result<Report> {
    // Order of the results of each extrinsic execution in the batch
    // is the same as in the input set of calls in the batch.
    // See: https://paritytech.github.io/substrate/master/src/pallet_utility/lib.rs.html#452-468
    match batch {
        Batch::UploadProgram(args) => {
            let args = args.into_iter().map(|v| v.into());

            let (ex_results, batch_block_hash) = api.upload_program_bytes_batch(args).await?;

            let mut init_messages = BTreeMap::new();

            for (i, r) in ex_results.into_iter().enumerate() {
                let call_id = i + 1;
                match r {
                    Ok((mid, pid)) => {
                        init_messages.insert(mid, (pid, call_id));
                    }
                    Err(e) => tracing::debug!("[Call with id: {call_id}] Failed: '{e:?}'"),
                }
            }

            let results: Result<Vec<(MessageId, Option<String>)>>;

            let now = utils::now();

            loop {
                let r = match api.events_since(batch_block_hash, 10).await {
                    Ok(mut v) => v.err_or_succeed_batch(init_messages.keys().cloned()).await,
                    Err(e) => Err(e),
                };

                if utils::now() - now > 1100 {
                    tracing::debug!("Timeout is reached while waiting for events");
                    results = Err(anyhow!("Out of time: probably blocks stopped.").into());
                    break;
                }

                if matches!(r, Err(Error::EventNotFoundInIterator)) {
                    continue;
                } else {
                    results = r;
                    break;
                }
            }

            let results = results?;

            let mut listener = api.subscribe().await?;
            let blocks_stopped = !listener.blocks_running().await?;

            let mut program_ids = BTreeSet::new();

            for (mid, maybe_err) in results {
                let (pid, call_id) = init_messages.remove(&mid).expect("Infallible");

                if let Some(expl) = maybe_err {
                    tracing::debug!("[Call with id: {call_id}]: Program with {pid:#.2} failed initialization on {mid:#.2} with a trap: '{expl}'");
                } else {
                    // TODO: handle case of "NotExecuted". It's not actual for init messages, but will be useful in future.
                    tracing::debug!("[Call with id: {call_id}]: {mid:#.2} successfully inited program with '{pid:#.2}'");
                    program_ids.insert(pid);
                }
            }

            Ok(Report {
                program_ids,
                blocks_stopped,
                codes: BTreeSet::new(),
            })
        }
        Batch::UploadCode(args) => {
            let args = args.into_iter().map(Into::<Vec<_>>::into);
            let (ex_results, _) = api.upload_code_batch(args).await?;

            let mut codes = BTreeSet::new();

            for (i, r) in ex_results.into_iter().enumerate() {
                let call_id = i + 1;
                match r {
                    Ok(code_id) => {
                        codes.insert(code_id);
                        tracing::debug!("[Call with id: {call_id}]: Successfully deployed code with id '{code_id}'");
                    }
                    Err(e) => tracing::debug!("[Call with id: {call_id}]: Failed '{e:?}'"),
                }
            }

            let mut listener = api.subscribe().await?;
            let blocks_stopped = !listener.blocks_running().await?;

            Ok(Report {
                program_ids: BTreeSet::new(),
                blocks_stopped,
                codes,
            })
        }
        Batch::SendMessage(args) => {
            let args = args.into_iter().map(|v| v.into());

            let (ex_results, batch_block_hash) = api.send_message_bytes_batch(args).await?;

            let mut handle_messages = BTreeMap::new();

            for (i, r) in ex_results.into_iter().enumerate() {
                let call_id = i + 1;
                match r {
                    Ok((mid, pid)) => {
                        handle_messages.insert(mid, (pid, call_id));
                    }
                    Err(e) => tracing::debug!("[Call with id: {call_id}]: Failed '{e:?}'"),
                }
            }

            let results: Result<Vec<(MessageId, Option<String>)>>;

            let now = utils::now();

            loop {
                let r = match api.events_since(batch_block_hash, 10).await {
                    Ok(mut v) => {
                        v.err_or_succeed_batch(handle_messages.keys().cloned())
                            .await
                    }
                    Err(e) => Err(e),
                };

                if utils::now() - now > 1100 {
                    tracing::debug!("Timeout is reached while waiting for events");
                    results = Err(anyhow!("Out of time: probably blocks stopped.").into());
                    break;
                }

                if matches!(r, Err(Error::EventNotFoundInIterator)) {
                    continue;
                } else {
                    results = r;
                    break;
                }
            }

            let results = results?;

            let mut listener = api.subscribe().await?;
            let blocks_stopped = !listener.blocks_running().await?;

            for (mid, maybe_err) in results {
                let (pid, call_id) = handle_messages.remove(&mid).expect("Infallible");

                if let Some(expl) = maybe_err {
                    tracing::debug!("[Call with id: {call_id}]: Message {mid:#.2} sent to program {pid:#.2} failed execution with a trap: '{expl}'");
                } else {
                    tracing::debug!("[Call with id: {call_id}]: Successfully executed {mid:#.2} message for program '{pid:#.2}'");
                }
            }

            Ok(Report {
                codes: BTreeSet::new(),
                program_ids: BTreeSet::new(),
                blocks_stopped,
            })
        }
        Batch::CreateProgram(args) => {
            let args = args.into_iter().map(|v| v.into());

            let (ex_results, batch_block_hash) = api.create_program_bytes_batch(args).await?;

            let mut init_messages = BTreeMap::new();

            for (i, r) in ex_results.into_iter().enumerate() {
                let call_id = i + 1;
                match r {
                    Ok((mid, pid)) => {
                        init_messages.insert(mid, (pid, call_id));
                    }
                    Err(e) => tracing::debug!("[Call with id: {call_id}]: Failed '{e:?}'"),
                }
            }

            let results: Result<Vec<(MessageId, Option<String>)>>;

            let now = utils::now();

            loop {
                let r = match api.events_since(batch_block_hash, 10).await {
                    Ok(mut v) => v.err_or_succeed_batch(init_messages.keys().cloned()).await,
                    Err(e) => Err(e),
                };

                if utils::now() - now > 1100 {
                    tracing::debug!("Timeout is reached while waiting for events");
                    results = Err(anyhow!("Out of time: probably blocks stopped.").into());
                    break;
                }

                if matches!(r, Err(Error::EventNotFoundInIterator)) {
                    continue;
                } else {
                    results = r;
                    break;
                }
            }

            let results = results?;

            let mut listener = api.subscribe().await?;
            let blocks_stopped = !listener.blocks_running().await?;

            let mut program_ids = BTreeSet::new();

            for (mid, maybe_err) in results {
                let (pid, call_id) = init_messages.remove(&mid).expect("Infallible");

                if let Some(expl) = maybe_err {
                    tracing::debug!("[Call with id: {call_id}]: Program with {pid:#.2} failed initialization on {mid:#.2} with a trap: '{expl}'");
                } else {
                    tracing::debug!("[Call with id: {call_id}]: {mid:#.2} successfully inited program with '{pid:#.2}'");
                    // TODO: handle case of "NotExecuted". It's not actual for init messages, but will be useful in future.
                    program_ids.insert(pid);
                }
            }

            Ok(Report {
                program_ids,
                blocks_stopped,
                codes: BTreeSet::new(),
            })
        }
    }
}
