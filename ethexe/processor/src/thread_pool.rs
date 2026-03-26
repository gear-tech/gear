// This file is part of Gear.
//
// Copyright (C) 2026 Gear Technologies Inc.
// SPDX-License-Identifier: GPL-3.0-or-later WITH Classpath-exception-2.0
//
// This program is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.
//
// This program is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE. See the
// GNU General Public License for more details.
//
// You should have received a copy of the GNU General Public License
// along with this program. If not, see <https://www.gnu.org/licenses/>.

//! Small custom thread pool interface, because `rayon` is too smart
//! and `threadpool` is not smart enough.

use std::{any::Any, env, num::NonZero, panic::AssertUnwindSafe, thread};

type Task = (
    Box<dyn FnOnce() -> Box<dyn Any + Send + 'static> + Send + 'static>,
    tokio::sync::oneshot::Sender<thread::Result<Box<dyn Any + Send + 'static>>>,
);

#[derive(Debug, Clone)]
pub struct ThreadPool {
    task_tx: crossbeam::channel::Sender<Task>,
}

impl ThreadPool {
    /// Creates a new thread pool.
    pub fn new() -> Self {
        let n_cpus = env::var("ETHEXE_PROCESSOR_NUM_THREADS")
            .ok()
            .and_then(|num| num.parse().ok())
            .or_else(|| thread::available_parallelism().ok())
            .map_or(1, NonZero::get);

        let (task_tx, task_rx) = crossbeam::channel::unbounded::<Task>();

        for _ in 0..n_cpus {
            let task_rx = task_rx.clone();

            thread::spawn(move || {
                loop {
                    let Ok((task, sender)) = task_rx.recv() else {
                        // All connected `ThreadPool` instances were dropped
                        break;
                    };

                    // Output receiver could be cancelled
                    let _ = sender.send(std::panic::catch_unwind(AssertUnwindSafe(task)));
                }
            });
        }

        Self { task_tx }
    }

    /// Spawns a given task.
    ///
    /// Returns `Ok(result)` if a worker successfully
    /// processed the task and `Err(panic_info)` if the worker panicked.
    ///
    /// # Panics
    ///
    /// Propagates panics from the worker thread to the main thread.
    ///
    /// Panics if worker thread dies despite using
    /// `std::panic::catch_unwind` around the handler.
    pub async fn spawn<F, R>(&self, f: F) -> R
    where
        F: FnOnce() -> R + Send + 'static,
        R: Send + 'static,
    {
        let (tx, rx) = tokio::sync::oneshot::channel();

        let f = Box::new(move || {
            let res = f();
            Box::new(res) as Box<_>
        });

        self.task_tx
            .try_send((f, tx))
            .expect("The channel is unbounded");

        let res = rx
            .await
            .expect("Worker thread has died")
            .unwrap_or_else(|err| std::panic::resume_unwind(err));
        *res.downcast::<R>().expect("Failed to downcast result")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use futures::{FutureExt, StreamExt, stream::FuturesOrdered};

    fn task(n: usize) -> String {
        "amogus".repeat(n)
    }

    #[tokio::test]
    async fn test_thread_pool() {
        let thread_pool = ThreadPool::new();

        assert_eq!(thread_pool.spawn(|| task(2)).await, "amogusamogus");

        assert_eq!(
            [0, 1, 2, 3]
                .into_iter()
                .map(|n| thread_pool.spawn(move || task(n)))
                .collect::<FuturesOrdered<_>>()
                .collect::<Vec<_>>()
                .await,
            vec![
                String::from(""),
                String::from("amogus"),
                String::from("amogusamogus"),
                String::from("amogusamogusamogus"),
            ]
        );

        let n_cpus = thread::available_parallelism().map_or(1, NonZero::get);

        // Ensure that panics don't break things
        for _ in 0..n_cpus * 2 {
            assert!(
                AssertUnwindSafe(thread_pool.spawn(|| task(usize::MAX)))
                    .catch_unwind()
                    .await
                    .is_err()
            )
        }
    }
}
