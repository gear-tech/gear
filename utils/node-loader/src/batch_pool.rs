use crate::{args::SeedVariant, utils::LoaderRng};
use context::{TaskContextUpdate, TasksContext};
use futures::{stream::FuturesUnordered, StreamExt};
use gear_program::api::Api;
use generators::{BatchGenerator, BatchGeneratorImpl};
use report::{BatchRunReport, TaskReporter};
use std::marker::PhantomData;
use task::Task;

mod context;
pub(crate) mod generators;
mod report;
mod task;

type Seed = u64;

pub(crate) struct BatchPool<Rng: LoaderRng> {
    pool_size: usize,
    batch_size: usize,
    tasks_context: TasksContext,
    gear_api: Api,
    _phantom: PhantomData<Rng>,
}

impl<Rng: LoaderRng> BatchPool<Rng> {
    pub(crate) fn new(pool_size: usize, batch_size: usize, gear_api: Api) -> Self {
        Self {
            pool_size,
            batch_size,
            tasks_context: TasksContext::new::<Rng>(),
            gear_api,
            _phantom: PhantomData,
        }
    }

    pub(crate) async fn run(&mut self, code_seed_type: Option<SeedVariant>) {
        let mut batches = FuturesUnordered::new();

        let seed = crate::utils::now();
        println!("Running task pool with seed {seed}");

        let mut batch_gen = BatchGeneratorImpl::<Rng>::new(
            seed,
            self.batch_size,
            self.tasks_context.clone(),
            code_seed_type,
        );

        while batches.len() != self.pool_size {
            let (batch_seed, batch) = batch_gen.generate();
            batches.push(run_batch(self.gear_api.clone(), batch_seed, batch));
        }

        while let Some(report) = batches.next().await {
            self.process_run_report(report);
            let (batch_seed, batch) = batch_gen.generate();
            batches.push(run_batch(self.gear_api.clone(), batch_seed, batch));
        }
    }

    fn process_run_report(&mut self, report: BatchRunReport) {
        // TODO DN
        let BatchRunReport {
            seed,
            reports,
            context_update,
        } = report;
        self.tasks_context.update(context_update);
        println!(
            "Task with seed (id) {:?} has finished. Showing report",
            seed
        );
        for report in reports {
            println!("{report}");
        }
    }
}

async fn run_batch(gear_api: Api, batch_seed: u64, batch: Vec<Task>) -> BatchRunReport {
    let mut pre_run_report = Vec::with_capacity(batch.len());
    let batch = batch
        .into_iter()
        .map(|task| {
            pre_run_report.push(task.report());
            task.into()
        })
        .collect::<Vec<_>>();
    let (context_update, post_run_report) = run_batch_impl(gear_api, batch).await.into();
    BatchRunReport::new(batch_seed, pre_run_report, post_run_report, context_update)
}

async fn run_batch_impl(
    _gear_api: Api,
    _calls: Vec<gear_client::GearClientCall>,
) -> gear_client::Report {
    todo!("Todo DN")
}

mod gear_client {
    // Todo DN

    use super::*;
    pub(super) struct GearClientCall;
    pub(super) struct Report;

    impl From<Report> for (TaskContextUpdate, super::report::PostRunReport) {
        fn from(_: gear_client::Report) -> Self {
            todo!("Todo DN")
        }
    }
}
