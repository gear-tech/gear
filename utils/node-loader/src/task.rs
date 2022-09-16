use std::fmt::Debug;

use futures::Future;
use tokio::task::JoinHandle;

pub(crate) struct TaskPool<T>(Vec<JoinHandle<T>>);

impl<T> TaskPool<T> {
    const POOL_SIZE: usize = 100;

    pub(crate) fn new() -> Self {
        Self(Vec::with_capacity(Self::POOL_SIZE))
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
