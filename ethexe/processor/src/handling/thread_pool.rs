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

use futures::prelude::*;
use std::{num::NonZero, panic::UnwindSafe, thread};

type Task<I, O> = (I, tokio::sync::oneshot::Sender<thread::Result<O>>);

/// Thread pool that handler tasks of type `I`
/// and produces outputs of type `O`.
#[derive(Debug, Clone)]
pub struct ThreadPool<I, O> {
    task_tx: crossbeam::channel::Sender<Task<I, O>>,
}

impl<I, O> ThreadPool<I, O>
where
    I: Send + UnwindSafe + 'static,
    O: Send + 'static,
{
    /// Creates a new thread pool.
    pub fn new<F>(handler: F) -> Self
    where
        F: FnMut(I) -> O + Send + Clone + UnwindSafe + 'static,
    {
        let n_cpus = thread::available_parallelism().map_or(1, NonZero::get);

        let (task_tx, task_rx) = crossbeam::channel::unbounded::<Task<I, O>>();

        for _ in 0..n_cpus {
            let task_rx = task_rx.clone();
            let handler = handler.clone();

            thread::spawn(move || {
                loop {
                    let Ok((task, sender)) = task_rx.recv() else {
                        // All connected `ThreadPool` instances were dropped
                        break;
                    };

                    let mut handler = handler.clone();

                    // Output receiver could be cancelled
                    let _ = sender.send(std::panic::catch_unwind(move || handler(task)));
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
    /// Panics if worker thread dies despite using
    /// `std::panic::catch_unwind` around the handler.
    pub async fn spawn(&self, input: I) -> thread::Result<O> {
        let (tx, rx) = tokio::sync::oneshot::channel();

        self.task_tx
            .try_send((input, tx))
            .expect("The channel is unbounded");

        rx.await.expect("Worker thread has died")
    }

    /// Spawns tasks from an iterator of inputs,
    /// producing a stream of outputs.
    ///
    /// The outputs are ordered the same as inputs.
    pub fn spawn_many<II: IntoIterator<Item = I>>(
        &self,
        input: II,
    ) -> impl Stream<Item = thread::Result<O>> {
        input
            .into_iter()
            .map(|input| self.spawn(input))
            .collect::<stream::FuturesOrdered<_>>()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_thread_pool() {
        let thread_pool = ThreadPool::new(|n| "amogus".repeat(n));

        assert_eq!(
            thread_pool.spawn(2).await.as_deref().ok(),
            Some("amogusamogus")
        );
        assert_eq!(
            thread_pool
                .spawn_many([0, 1, 2, 3])
                .map(Result::ok)
                .collect::<Vec<_>>()
                .await,
            vec![
                Some("".into()),
                Some("amogus".into()),
                Some("amogusamogus".into()),
                Some("amogusamogusamogus".into()),
            ]
        );

        let n_cpus = thread::available_parallelism().map_or(1, NonZero::get);

        // Ensure that panics don't break things
        for _ in 0..n_cpus * 2 {
            assert!(thread_pool.spawn(usize::MAX).await.is_err())
        }
    }
}
