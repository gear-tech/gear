use std::marker::PhantomData;

use gear_program::api::Api;
use tokio::task::JoinHandle;

use crate::{args::SeedVariant, reporter::SomeReporter};
use upload_program::UploadProgramTaskGen;

pub(crate) mod generators;
mod upload_program;

pub(crate) struct TaskPool<Rng: crate::Rng> {
    up_task_gen: UploadProgramTaskGen,
    tasks: Vec<JoinHandle<SomeReporter>>,
    _phantom: PhantomData<Rng>
}

impl<Rng: crate::Rng> TaskPool<Rng> {
    pub(crate) const MIN_SIZE: usize = 1;
    pub(crate) const MAX_SIZE: usize = 100;

    pub(crate) fn try_new(
        size: usize,
        seed_variant: Option<SeedVariant>,
        gear_api: Api,
    ) -> Result<Self, String> 
    {
        if size >= Self::MIN_SIZE && size <= Self::MAX_SIZE {
            Ok(Self {
                up_task_gen: UploadProgramTaskGen::try_new(
                    gear_api,
                    generators::get_some_seed_generator::<Rng>(seed_variant),
                ),
                tasks: Vec::with_capacity(size),
                _phantom: PhantomData
            })
        } else {
            Err(format!(
                "Can't create task pool with such size {size:?}. \
                Allowed minimum size is {:?} and maximum {:?}",
                Self::MIN_SIZE,
                Self::MAX_SIZE,
            ))
        }
    }

    pub(crate) async fn run(&mut self) -> Result<Vec<SomeReporter>, ()> {
        let TaskPool { tasks, .. } = self;
        let mut results = Vec::with_capacity(tasks.capacity());

        tasks.clear();
        while tasks.len() != tasks.capacity() {
            let task = tokio::spawn(self.up_task_gen.gen::<Rng>());
            tasks.push(task)
        }

        for task in tasks {
            let res = task.await.map_err(|_| ())?;
            results.push(res);
        }

        Ok(results)
    }
}
