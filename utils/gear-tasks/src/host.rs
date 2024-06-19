// This file is part of Gear.
//
// Copyright (C) 2024 Gear Technologies Inc.
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

use crate::{JoinError, JoinHandle, JoinResult};
use futures_executor::ThreadPool;
use gear_tasks_runtime_api::GearTasksApi;
use sc_client_api::UsageProvider;
use sp_api::{ApiExt, BlockT, HashingFor, ProvideRuntimeApi};
use sp_state_machine::OverlayedChanges;
use std::{
    any::Any,
    collections::HashMap,
    marker::PhantomData,
    sync::{mpsc, Arc, OnceLock},
};

static RUNNER_TX: OnceLock<mpsc::Sender<TaskInfo>> = OnceLock::new();

enum TaskInfo {
    ReInit {
        tasks: u8,
    },
    Spawn {
        overlayed_changes: Box<dyn Any + Send>,
        func_ref: u64,
        payload: Vec<u8>,
        rx: mpsc::SyncSender<JoinResult>,
    },
}

impl TaskInfo {
    fn send_to_runner(self) {
        RUNNER_TX
            .get()
            .expect("`GearTasksRunner` is not spawned")
            .send(self)
            .expect("`GearTasksRunner` has been dropped")
    }
}

pub trait RuntimeSetOverlayedChanges<B: BlockT> {
    fn set_overlayed_changes(&mut self, changes: OverlayedChanges<HashingFor<B>>);
}

sp_externalities::decl_extension! {
    /// Set only by `GearTasksRunner` and checked by `gear_tasks::check_context()` host call,
    /// so no one can call API outside.
    pub(crate) struct GearTasksContextExt;
}

pub struct GearTasksRunner<RA, Block> {
    runtime_api_provider: Arc<RA>,
    rx: mpsc::Receiver<TaskInfo>,
    thread_pool: Option<ThreadPool>,
    _block: PhantomData<Block>,
}

impl<RA, Block> GearTasksRunner<RA, Block>
where
    RA: ProvideRuntimeApi<Block> + UsageProvider<Block> + Send + Sync + 'static,
    RA::Api: GearTasksApi<Block> + RuntimeSetOverlayedChanges<Block>,
    Block: BlockT,
{
    pub fn new(client: Arc<RA>) -> Self {
        let (tx, rx) = mpsc::channel();
        assert!(
            RUNNER_TX.get().is_none(),
            "`GearTasksRunner` initialized twice"
        );
        let _tx = RUNNER_TX.get_or_init(move || tx);

        Self {
            runtime_api_provider: client,
            rx,
            thread_pool: None,
            _block: PhantomData,
        }
    }

    pub async fn run(mut self) {
        for info in self.rx {
            match info {
                TaskInfo::ReInit { tasks } => {
                    self.thread_pool = Some(
                        ThreadPool::builder()
                            .pool_size(tasks as usize)
                            .name_prefix("gear-tasks-")
                            .create()
                            .expect("Thread pool creation failed"),
                    );
                }
                TaskInfo::Spawn {
                    overlayed_changes,
                    func_ref,
                    payload,
                    rx,
                } => {
                    let client = self.runtime_api_provider.clone();
                    let thread_pool = self
                        .thread_pool
                        .as_ref()
                        .expect("`TaskInfo::ReInit` has never been sent");
                    thread_pool.spawn_ok(async move {
                        let mut runtime_api = client.runtime_api();
                        runtime_api.register_extension(GearTasksContextExt);

                        let overlayed_changes = overlayed_changes
                            .downcast::<OverlayedChanges<HashingFor<Block>>>()
                            .expect("`Externalities::gear_overlayed_changes()` implementation is invalid");
                        runtime_api.set_overlayed_changes(*overlayed_changes);

                        let block_hash = client.usage_info().chain.best_hash;

                        let res = runtime_api
                            .execute_task(block_hash, func_ref, payload)
                            .map_err(|e| JoinError::RuntimeApi(e.to_string()));

                        rx.send(res)
                            .expect("`TaskSpawner` dropped before task completion and `join()` on it")
                    });
                }
            }
        }
    }
}

sp_externalities::decl_extension! {
    pub(crate) struct TaskSpawnerExt(TaskSpawner);
}

impl TaskSpawnerExt {
    pub(crate) fn new(tasks: u8) -> Self {
        Self(TaskSpawner::new(tasks))
    }
}

pub struct TaskSpawner {
    counter: u64,
    tasks: HashMap<u64, mpsc::Receiver<JoinResult>>,
}

impl TaskSpawner {
    fn new(tasks: u8) -> Self {
        TaskInfo::ReInit { tasks }.send_to_runner();

        Self {
            counter: 0,
            tasks: HashMap::new(),
        }
    }

    pub(crate) fn spawn(
        &mut self,
        overlayed_changes: Box<dyn Any + Send>,
        func_ref: u64,
        payload: Vec<u8>,
    ) -> JoinHandle {
        let handle = self.counter;
        self.counter += 1;

        let (rx, tx) = mpsc::sync_channel(1);

        TaskInfo::Spawn {
            overlayed_changes,
            func_ref,
            payload,
            rx,
        }
        .send_to_runner();

        self.tasks.insert(handle, tx);
        JoinHandle { inner: handle }
    }

    pub(crate) fn join(&mut self, handle: JoinHandle) -> JoinResult {
        let tx = self
            .tasks
            .remove(&handle.inner)
            .expect("`JoinHandle` is duplicated so task not found");
        tx.recv()
            .expect("Sender has been disconnected which means thread was somehow terminated")
    }
}

impl Drop for TaskSpawner {
    fn drop(&mut self) {
        assert_eq!(self.tasks.len(), 0, "Not every task has been joined");
    }
}
