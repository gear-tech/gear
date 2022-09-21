use std::{
    pin::Pin,
    sync::{Arc, Mutex},
};

use futures::stream::FuturesUnordered;
use gear_program::api::Api;
use rand::{rngs::SmallRng, seq::SliceRandom, RngCore};
use tokio::{
    sync::mpsc::{self, Receiver},
    task::{JoinError, JoinHandle},
};
use anyhow::{Result, anyhow};

use crate::{args::SeedVariant, reporter::SomeReporter};
use upload_program::UploadProgramTaskGen;

use self::{
    generators::{FutureSomeReporter, TaskGen},
    upload_code::UploadCodeTaskGen,
};

pub(crate) mod generators;
mod upload_code;
mod upload_program;

type TaskGenVec = Vec<Box<dyn TaskGen<Output = FutureSomeReporter> + Send + Sync>>;

pub(crate) struct TaskPool {
    rx: Option<Receiver<JoinHandle<SomeReporter>>>,
    tasks: FuturesUnordered<JoinHandle<SomeReporter>>,
    seed_gen: Arc<Mutex<Box<dyn RngCore + Send + Sync>>>,
}

impl TaskPool {
    pub(crate) const MIN_SIZE: usize = 1;
    pub(crate) const MAX_SIZE: usize = 100;

    pub(crate) fn try_new(size: usize, seed_variant: Option<SeedVariant>) -> Result<Self> {
        if size >= Self::MIN_SIZE && size <= Self::MAX_SIZE {
            let seed_gen = Arc::new(Mutex::new(generators::get_some_seed_generator::<SmallRng>(
                seed_variant,
            )));

            Ok(Self {
                rx: None,
                tasks: FuturesUnordered::<JoinHandle<SomeReporter>>::new(),
                seed_gen,
            })
        } else {
            Err(anyhow!(format!(
                "Can't create task pool with such size {size:?}. \
                Allowed minimum size is {:?} and maximum {:?}",
                Self::MIN_SIZE,
                Self::MAX_SIZE,
            )))
        }
    }

    pub(crate) async fn run(&mut self, gear_api: Api) {
        let seed_gen = Arc::clone(&self.seed_gen);
        let (tx, rx) = mpsc::channel::<JoinHandle<SomeReporter>>(10);

        tokio::spawn(async move {
            let gens: TaskGenVec = vec![
                Box::new(UploadProgramTaskGen::try_new(
                    gear_api.clone(),
                    Arc::clone(&seed_gen),
                )),
                Box::new(UploadCodeTaskGen::try_new(gear_api, Arc::clone(&seed_gen))),
            ];

            loop {
                let task_gen = gens.choose(&mut rand::thread_rng()).ok_or(()).unwrap();
                if let Err(e) = tx.send(tokio::spawn(task_gen.gen())).await{
                    println!("Receiver is closed: {e}");
                };
            }
        });
        self.rx = Some(rx);
    }
}

impl futures::stream::Stream for TaskPool {
    type Item = Result<SomeReporter, JoinError>;

    fn poll_next(
        self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<Option<Self::Item>> {
        let s = Pin::get_mut(self);
        if let Some(rx) = &mut s.rx {
            while let Some(f) = rx.blocking_recv() {
                s.tasks.push(f);
                if s.tasks.len() >= 100 {
                    break;
                };
            }
        }
        match Pin::new(&mut s.tasks).poll_next(cx) {
            std::task::Poll::Pending => std::task::Poll::Pending,
            std::task::Poll::Ready(r) => std::task::Poll::Ready(r),
        }
    }
}
