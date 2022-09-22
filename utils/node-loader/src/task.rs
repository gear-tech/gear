use std::{
    marker::PhantomData,
    sync::{Arc, Mutex},
};

use anyhow::{anyhow, Result};
use futures::{stream::FuturesUnordered, StreamExt};
use gear_program::api::Api;
use rand::seq::SliceRandom;
use tokio::task::JoinHandle;

use crate::{args::SeedVariant, reporter::SomeReporter};
use upload_program::UploadProgramTaskGen;

use self::{
    generators::{FutureSomeReporter, TaskGen},
    upload_code::UploadCodeTaskGen,
};

pub(crate) mod generators;
mod upload_code;
mod upload_program;

type TaskGenVec<Rng> = Vec<Box<dyn TaskGen<Rng, Output = FutureSomeReporter> + Send + Sync>>;

pub(crate) struct TaskPool<Rng: crate::Rng> {
    gens: TaskGenVec<Rng>,
    tasks: FuturesUnordered<JoinHandle<SomeReporter>>,
    _phantom: PhantomData<Rng>,
}

impl<Rng: crate::Rng> TaskPool<Rng> {
    pub(crate) const MIN_SIZE: usize = 1;
    pub(crate) const MAX_SIZE: usize = 100;

    pub(crate) fn try_new(
        gear_api: Api,
        size: usize,
        seed_variant: Option<SeedVariant>,
    ) -> Result<Self> {
        if size >= Self::MIN_SIZE && size <= Self::MAX_SIZE {
            let seed_gen = Arc::new(Mutex::new(generators::get_some_seed_generator::<Rng>(
                seed_variant,
            )));

            let gens: TaskGenVec<Rng> = vec![
                Box::new(UploadProgramTaskGen::try_new(
                    gear_api.clone(),
                    Arc::clone(&seed_gen),
                )),
                Box::new(UploadCodeTaskGen::try_new(gear_api, Arc::clone(&seed_gen))),
            ];

            Ok(Self {
                gens,
                tasks: FuturesUnordered::<JoinHandle<SomeReporter>>::new(),
                _phantom: PhantomData,
            })
        } else {
            Err(anyhow!(
                "Can't create task pool with such size {size:?}. \
                Allowed minimum size is {:?} and maximum {:?}",
                Self::MIN_SIZE,
                Self::MAX_SIZE,
            ))
        }
    }

    pub(crate) async fn run(&mut self) {
        // fill pool
        while self.tasks.len() != 100 {
            self.tasks.push(tokio::spawn(self.gen_task()))
        }

        loop {
            match self.tasks.next().await {
                Some(r) => {
                    if let Ok(reporter) = r {
                        if let Err(e) = reporter.report() {
                            println!("Reporter error: {e}")
                        }
                    } else {
                        println!("Task join error");
                    }
                    self.tasks.push(tokio::spawn(self.gen_task()))
                }
                None => continue,
            }
        }
    }

    fn gen_task(&self) -> FutureSomeReporter {
        let task_gen = self.gens.choose(&mut rand::thread_rng()).ok_or(()).unwrap();
        task_gen.gen()
    }
}
