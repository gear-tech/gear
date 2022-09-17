use std::fmt::Debug;

use futures::Future;
use tokio::task::JoinHandle;

pub(crate) struct TaskPool<T>(Vec<JoinHandle<T>>);

impl<T> TaskPool<T> {
    pub(crate) const MIN_SIZE: usize = 1;
    pub(crate) const MAX_SIZE: usize = 100;

    pub(crate) fn try_new(size: usize) -> Result<Self, String> {
        if size >= Self::MIN_SIZE && size <= Self::MAX_SIZE {
            Ok(Self(Vec::with_capacity(size)))
        } else {
            Err(format!(
                "Can't create task pool with such size {size:?}. \
                Allowed minimum size is {:?} and maximum {:?}",
                Self::MIN_SIZE,
                Self::MAX_SIZE,
            ))
        }
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
