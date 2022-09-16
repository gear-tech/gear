use std::fmt::Debug;

use tokio::task::JoinHandle;
use futures::Future;

use super::args::Params;

pub(crate) struct TaskPool<T>(Vec<JoinHandle<T>>);

impl<T> TaskPool<T> {
    pub(crate) fn new(params: &Params) -> Self {
        Self(Vec::with_capacity(params.workers as usize))
    }

    pub(crate) async fn run<Job>(&mut self, job_wrap: impl Fn() -> Job) -> Result<Vec<T>, ()>
    where
        Job: Future<Output = T> + Send + 'static,
        T: Debug + Send + 'static,
    {
        let Self(tasks) = self;
        let mut results = Vec::with_capacity(tasks.capacity());

        tasks.clear();
        while tasks.len() != tasks.capacity() {
            let task = tokio::spawn(job_wrap());
            tasks.push(task)
        }

        for task in tasks {
            let res = task.await.map_err(|_| ())?;
            results.push(res);
        }

        Ok(results)
    }
}