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

use std::num::NonZero;

type Task<I, O> = (I, tokio::sync::oneshot::Sender<O>);

/// Thread pool that handler tasks of type `I`
/// and produces outputs of type `O`.
#[derive(Debug, Clone)]
pub struct ThreadPool<I, O, F = fn(I) -> O> {
    task_tx: crossbeam::channel::Sender<Task<I, O>>,
    task_rx: crossbeam::channel::Receiver<Task<I, O>>,
    handler: F,
}

impl<I, O, F> ThreadPool<I, O, F>
where
    I: Send + 'static,
    O: Send + 'static,
    F: FnMut(I) -> O + Send + Clone + 'static,
{
    /// Creates a new thread pool.
    pub fn new(handler: F) -> Self {
        let n_cpus = std::thread::available_parallelism().map_or(1, NonZero::get);

        let (task_tx, task_rx) = crossbeam::channel::unbounded();

        let thread_pool = Self {
            task_tx,
            task_rx,
            handler,
        };

        for _ in 0..n_cpus {
            thread_pool.spawn_worker();
        }

        thread_pool
    }

    fn spawn_worker(&self) {
        let task_rx = self.task_rx.clone();
        let handler = self.handler.clone();

        std::thread::spawn(move || {
            loop {
                let Ok((task, sender)) = task_rx.recv() else {
                    // All connected `ThreadPool` instances were dropped
                    break;
                };

                let mut handler = handler.clone();

                // Output receiver could be cancelled
                let _ = sender.send(handler(task));
            }
        });
    }

    /// Spawns a given task.
    ///
    /// Returns `Some(result)` if a worker successfully
    /// processed the task and `None` if the worker panicked.
    pub fn spawn_task(&self, input: I) -> impl Future<Output = Option<O>> {
        let (tx, rx) = tokio::sync::oneshot::channel();

        self.task_tx
            .try_send((input, tx))
            .expect("The channel is unbounded");

        async move {
            rx.await
                .inspect_err(|_| {
                    // Respawn panicked thread
                    self.spawn_worker();
                })
                .ok()
        }
    }
}
