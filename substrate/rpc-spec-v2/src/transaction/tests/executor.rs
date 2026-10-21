// This file is part of Substrate.

// Copyright (C) Parity Technologies (UK) Ltd.
// SPDX-License-Identifier: GPL-3.0-or-later WITH Classpath-exception-2.0

use sp_core::{testing::TaskExecutor, traits::SpawnNamed};
use std::sync::{atomic::AtomicUsize, Arc};
use tokio::sync::mpsc;

/// Wrap the `TaskExecutor` to know when the broadcast future is dropped.
#[derive(Clone)]
pub struct TaskExecutorBroadcast {
    executor: TaskExecutor,
    sender: mpsc::UnboundedSender<()>,
    num_tasks: Arc<AtomicUsize>,
}

/// The channel that receives events when the broadcast futures are dropped.
pub type TaskExecutorRecv = mpsc::UnboundedReceiver<()>;

/// The state of the `TaskExecutorBroadcast`.
pub struct TaskExecutorState {
    pub recv: TaskExecutorRecv,
    pub num_tasks: Arc<AtomicUsize>,
}

impl TaskExecutorState {
    pub fn num_tasks(&self) -> usize {
        self.num_tasks.load(std::sync::atomic::Ordering::Acquire)
    }
}

impl TaskExecutorBroadcast {
    /// Construct a new `TaskExecutorBroadcast` and a receiver to know when the broadcast futures
    /// are dropped.
    pub fn new() -> (Self, TaskExecutorState) {
        let (sender, recv) = mpsc::unbounded_channel();
        let num_tasks = Arc::new(AtomicUsize::new(0));

        (
            Self {
                executor: TaskExecutor::new(),
                sender,
                num_tasks: num_tasks.clone(),
            },
            TaskExecutorState { recv, num_tasks },
        )
    }
}

impl SpawnNamed for TaskExecutorBroadcast {
    fn spawn(
        &self,
        name: &'static str,
        group: Option<&'static str>,
        future: futures::future::BoxFuture<'static, ()>,
    ) {
        let sender = self.sender.clone();
        let num_tasks = self.num_tasks.clone();

        let future = Box::pin(async move {
            num_tasks.fetch_add(1, std::sync::atomic::Ordering::AcqRel);
            future.await;
            num_tasks.fetch_sub(1, std::sync::atomic::Ordering::AcqRel);

            let _ = sender.send(());
        });

        self.executor.spawn(name, group, future)
    }

    fn spawn_blocking(
        &self,
        name: &'static str,
        group: Option<&'static str>,
        future: futures::future::BoxFuture<'static, ()>,
    ) {
        let sender = self.sender.clone();
        let num_tasks = self.num_tasks.clone();

        let future = Box::pin(async move {
            num_tasks.fetch_add(1, std::sync::atomic::Ordering::AcqRel);
            future.await;
            num_tasks.fetch_sub(1, std::sync::atomic::Ordering::AcqRel);

            let _ = sender.send(());
        });

        self.executor.spawn_blocking(name, group, future)
    }
}
