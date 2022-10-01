use crate::{
    args::SeedVariant,
    utils::{self, LoaderRng},
};
use anyhow::anyhow;
use batch::Batch;
use context::Context;
use futures::{stream::FuturesUnordered, StreamExt};
use gclient::{Error, EventProcessor, GearApi, Result};
use gear_core::ids::MessageId;
use generators::BatchGenerator;
use report::{BatchReporter, BatchRunReport};
use std::{
    collections::{BTreeMap, BTreeSet},
    fs::{self, File},
    io::Write,
    marker::PhantomData,
};

use self::{batch::BatchWithSeed, report::Report};

mod batch;
mod context;
pub mod generators;
mod report;

type Seed = u64;

pub struct BatchPool<Rng: LoaderRng> {
    api: GearApi,
    pool_size: usize,
    batch_size: usize,
    tasks_context: Context,
    _phantom: PhantomData<Rng>,
}

impl<Rng: LoaderRng> BatchPool<Rng> {
    pub fn new(api: GearApi, pool_size: usize, batch_size: usize) -> Self {
        Self {
            api,
            pool_size,
            batch_size,
            tasks_context: Context::new(),
            _phantom: PhantomData,
        }
    }

    pub async fn run(&mut self, code_seed_type: Option<SeedVariant>) -> Result<()> {
        let mut batches = FuturesUnordered::new();

        let seed = utils::now();
        let info = format!("Running task pool with seed {seed}\n\n");
        println!("{info}");

        fs::write(".log", info.as_bytes()).expect("Failed to write into file");

        let mut batch_gen = BatchGenerator::<Rng>::new(
            seed,
            self.batch_size,
            self.tasks_context.clone(),
            code_seed_type,
        );

        let mut num = self.api.rpc_nonce().await?;

        while batches.len() != self.pool_size {
            let batch_with_seed = batch_gen.generate();

            let mut api = self.api.clone();
            api.set_nonce(num);
            num += 1;

            batches.push(run_batch(api, batch_with_seed));
        }

        while let Some(report) = batches.next().await {
            self.process_run_report(report);
            let batch_with_seed = batch_gen.generate();

            let mut api = self.api.clone();
            api.set_nonce(num);
            num += 1;

            batches.push(run_batch(api, batch_with_seed));
        }

        unreachable!()
    }

    fn process_run_report(&mut self, report: BatchRunReport) {
        let BatchRunReport {
            reports,
            context_update,
            blocks_stopped,
        } = report;

        self.tasks_context.update(context_update);

        let res = format!("\n{}\n", reports.join("\n"));
        println!("{res}");

        let mut file = File::options()
            .write(true)
            .append(true)
            .create(true)
            .open(".log")
            .expect("Failed to create a file");

        file.write_all(res.as_bytes())
            .expect("Failed to write into file");

        assert!(!blocks_stopped);
    }
}

async fn run_batch(api: GearApi, batch: BatchWithSeed) -> BatchRunReport {
    let pre_run_report = batch.report();

    match run_batch_impl(api, batch.into()).await {
        Ok(report) => BatchRunReport::new(pre_run_report, report),
        Err(err) => BatchRunReport::from_err(pre_run_report, err),
    }
}

async fn run_batch_impl(api: GearApi, batch: Batch) -> Result<Report> {
    let mut logs = vec![];

    match batch {
        Batch::UploadProgram(args) => {
            let args = args.into_iter().map(|v| v.into());

            let (ex_results, batch_block_hash) = api.upload_program_bytes_batch(args).await?;

            let mut init_messages = BTreeMap::new();

            for r in ex_results {
                match r {
                    Ok((mid, pid)) => {
                        init_messages.insert(mid, pid);
                    }
                    Err(e) => logs.push(format!(
                        "[#{:<2}] Extrinsic failure: '{:?}'",
                        logs.len() + 1,
                        e
                    )),
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
                let pid = init_messages.remove(&mid).expect("Infallible");

                if let Some(expl) = maybe_err {
                    logs.push(format!("[#{:<2}] Program with {pid:#.2} failed initialization on {mid:#.2} with a trap: '{expl}'", logs.len() + 1))
                } else {
                    // TODO: handle case of "NotExecuted". It's not actual for init messages, but will be useful in future.
                    logs.push(format!(
                        "[#{:<2}] {mid:#.2} successfully inited program with '{pid:#.2}'",
                        logs.len() + 1
                    ));
                    program_ids.insert(pid);
                }
            }

            Ok(Report {
                logs,
                program_ids,
                blocks_stopped,
                codes: BTreeSet::new(),
            })
        }
        Batch::UploadCode(args) => {
            let args = args.into_iter().map(|v| Into::<Vec<_>>::into(v));
            let (ex_results, _) = api.upload_code_batch(args).await?;

            let mut codes = BTreeSet::new();

            for r in ex_results {
                match r {
                    Ok(code_id) => {
                        codes.insert(code_id);
                        logs.push(format!(
                            "[#{:<2}] Successfully deployed code with id {code_id}",
                            logs.len() + 1,
                        ));
                    }
                    Err(e) => logs.push(format!(
                        "[#{:<2}] Extrinsic failure: '{:?}'",
                        logs.len() + 1,
                        e
                    )),
                }
            }

            let mut listener = api.subscribe().await?;
            let blocks_stopped = !listener.blocks_running().await?;

            Ok(Report { logs, program_ids: BTreeSet::new(), blocks_stopped, codes })
        }
        _ => unimplemented!(),
    }
}
