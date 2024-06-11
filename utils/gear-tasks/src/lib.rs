/*
 *   Copyright (c) 2024
 *   All rights reserved.
 */
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

#![cfg_attr(not(feature = "std"), no_std)]

extern crate alloc;

#[cfg(feature = "std")]
pub use inner::GearTasksRunner;
pub use inner::{spawn, JoinHandle};

use alloc::vec::Vec;
use sp_externalities::ExternalitiesExt;
use sp_runtime_interface::runtime_interface;

const TASKS_AMOUNT: usize = 4;

/// WASM host functions for managing tasks.
#[runtime_interface]
pub trait GearTasks {
    fn init(&mut self) {
        self.register_extension(inner::TaskSpawnerExt::default())
            .expect("`GearTasks` initialized twice");
    }

    fn spawn(&mut self, func_ref: u64, payload: Vec<u8>) -> u64 {
        let spawner = self
            .extension::<inner::TaskSpawnerExt>()
            .expect("Cannot spawn without dynamic runtime dispatcher (TaskSpawnerExt)");
        let handle = spawner.spawn_wasm(func_ref, payload);
        handle.inner
    }

    fn join(&mut self, handle: u64) -> Vec<u8> {
        let spawner = self
            .extension::<inner::TaskSpawnerExt>()
            .expect("Cannot join without dynamic runtime dispatcher (TaskSpawnerExt)");
        spawner.join(JoinHandle { inner: handle })
    }
}

#[cfg(feature = "std")]
mod inner {
    use super::*;

    use futures_executor::ThreadPool;
    use gear_tasks_runtime_api::GearTasksApi;
    use sc_client_api::UsageProvider;
    use sp_api::ProvideRuntimeApi;
    use sp_externalities::{Error, Extension, ExtensionStore, Externalities, MultiRemovalResults};
    use std::{
        any::{Any, TypeId},
        collections::HashMap,
        marker::PhantomData,
        sync::{mpsc, Arc, OnceLock},
    };

    struct NoStorageExternalities;

    impl Externalities for NoStorageExternalities {
        fn set_offchain_storage(&mut self, _key: &[u8], _value: Option<&[u8]>) {
            panic!("`Externalities::set_offchain_storage()` is not supported")
        }

        fn storage(&self, _key: &[u8]) -> Option<Vec<u8>> {
            panic!("`Externalities::storage()` is not supported")
        }

        fn storage_hash(&self, _key: &[u8]) -> Option<Vec<u8>> {
            panic!("`Externalities::storage_hash()` is not supported")
        }

        fn child_storage_hash(
            &self,
            _child_info: &sp_storage::ChildInfo,
            _key: &[u8],
        ) -> Option<Vec<u8>> {
            panic!("`Externalities::child_storage_hash()` is not supported")
        }

        fn child_storage(
            &self,
            _child_info: &sp_storage::ChildInfo,
            _key: &[u8],
        ) -> Option<Vec<u8>> {
            panic!("`Externalities::child_storage()` is not supported")
        }

        fn next_storage_key(&self, _key: &[u8]) -> Option<Vec<u8>> {
            panic!("`Externalities::next_storage_key()` is not supported")
        }

        fn next_child_storage_key(
            &self,
            _child_info: &sp_storage::ChildInfo,
            _key: &[u8],
        ) -> Option<Vec<u8>> {
            panic!("`Externalities::next_child_storage_key()` is not supported")
        }

        fn kill_child_storage(
            &mut self,
            _child_info: &sp_storage::ChildInfo,
            _maybe_limit: Option<u32>,
            _maybe_cursor: Option<&[u8]>,
        ) -> MultiRemovalResults {
            panic!("`Externalities::kill_child_storage()` is not supported")
        }

        fn clear_prefix(
            &mut self,
            _prefix: &[u8],
            _maybe_limit: Option<u32>,
            _maybe_cursor: Option<&[u8]>,
        ) -> MultiRemovalResults {
            panic!("`Externalities::clear_prefix()` is not supported")
        }

        fn clear_child_prefix(
            &mut self,
            _child_info: &sp_storage::ChildInfo,
            _prefix: &[u8],
            _maybe_limit: Option<u32>,
            _maybe_cursor: Option<&[u8]>,
        ) -> MultiRemovalResults {
            panic!("`Externalities::clear_child_prefix()` is not supported")
        }

        fn place_storage(&mut self, _key: Vec<u8>, _value: Option<Vec<u8>>) {
            panic!("`Externalities::place_storage()` is not supported")
        }

        fn place_child_storage(
            &mut self,
            _child_info: &sp_storage::ChildInfo,
            _key: Vec<u8>,
            _value: Option<Vec<u8>>,
        ) {
            panic!("`Externalities::place_child_storage()` is not supported")
        }

        fn storage_root(&mut self, _state_version: sp_storage::StateVersion) -> Vec<u8> {
            panic!("`Externalities::storage_root()` is not supported")
        }

        fn child_storage_root(
            &mut self,
            _child_info: &sp_storage::ChildInfo,
            _state_version: sp_storage::StateVersion,
        ) -> Vec<u8> {
            panic!("`Externalities::child_storage_root()` is not supported")
        }

        fn storage_append(&mut self, _key: Vec<u8>, _value: Vec<u8>) {
            panic!("`Externalities::storage_append()` is not supported")
        }

        fn storage_start_transaction(&mut self) {
            panic!("`Externalities::storage_start_transaction()` is not supported")
        }

        fn storage_rollback_transaction(&mut self) -> Result<(), ()> {
            panic!("`Externalities::storage_rollback_transaction()` is not supported")
        }

        fn storage_commit_transaction(&mut self) -> Result<(), ()> {
            panic!("`Externalities::storage_commit_transaction()` is not supported")
        }

        fn wipe(&mut self) {
            panic!("`Externalities::wipe()` is not supported")
        }

        fn commit(&mut self) {
            panic!("`Externalities::commit()` is not supported")
        }

        fn read_write_count(&self) -> (u32, u32, u32, u32) {
            panic!("`Externalities::read_write_count()` is not supported")
        }

        fn reset_read_write_count(&mut self) {
            panic!("`Externalities::reset_read_write_count()` is not supported")
        }

        fn get_whitelist(&self) -> Vec<sp_storage::TrackedStorageKey> {
            panic!("`Externalities::get_whitelist()` is not supported")
        }

        fn set_whitelist(&mut self, _new: Vec<sp_storage::TrackedStorageKey>) {
            panic!("`Externalities::set_whitelist()` is not supported")
        }

        fn get_read_and_written_keys(&self) -> Vec<(Vec<u8>, u32, u32, bool)> {
            panic!("`Externalities::get_read_and_written_keys()` is not supported")
        }
    }

    impl ExtensionStore for NoStorageExternalities {
        fn extension_by_type_id(&mut self, _type_id: TypeId) -> Option<&mut dyn Any> {
            panic!("`ExternalitiesStore::extension_by_type_id()` is not supported")
        }

        fn register_extension_with_type_id(
            &mut self,
            _type_id: TypeId,
            _extension: Box<dyn Extension>,
        ) -> Result<(), Error> {
            panic!("`ExternalitiesStore::register_extension_with_type_id()` is not supported")
        }

        fn deregister_extension_by_type_id(&mut self, _type_id: TypeId) -> Result<(), Error> {
            panic!("`ExternalitiesStore::deregister_extension_by_type_id()` is not supported")
        }
    }

    static TX: OnceLock<mpsc::Sender<TaskEvent>> = OnceLock::new();

    enum TaskEvent {
        SpawnWasm {
            func_ref: u64,
            payload: Vec<u8>,
            rx: mpsc::SyncSender<Vec<u8>>,
        },
        SpawnNative {
            func_ref: fn(Vec<u8>) -> Vec<u8>,
            payload: Vec<u8>,
            rx: mpsc::SyncSender<Vec<u8>>,
        },
    }

    pub struct GearTasksRunner<RA, Block: sp_api::BlockT> {
        runtime_api_provider: Arc<RA>,
        rx: mpsc::Receiver<TaskEvent>,
        thread_pool: ThreadPool,
        _block: PhantomData<Block>,
    }

    impl<RA, Block> GearTasksRunner<RA, Block>
    where
        RA: ProvideRuntimeApi<Block> + UsageProvider<Block> + Send + Sync + 'static,
        RA::Api: GearTasksApi<Block>,
        Block: sp_api::BlockT,
    {
        pub fn new(client: Arc<RA>) -> Self {
            let (tx, rx) = mpsc::channel();
            let _tx = TX.get_or_init(move || tx);

            log::error!("TX inited");

            Self {
                runtime_api_provider: client,
                rx,
                thread_pool: ThreadPool::builder()
                    .pool_size(TASKS_AMOUNT)
                    .name_prefix("gear-tasks-")
                    .create()
                    .expect("Thread pool creation failed"),
                _block: PhantomData,
            }
        }

        pub async fn run(self) {
            log::error!("RUN started");

            for event in self.rx {
                match event {
                    TaskEvent::SpawnWasm {
                        func_ref,
                        payload,
                        rx,
                    } => {
                        let client = self.runtime_api_provider.clone();
                        self.thread_pool.spawn_ok(async move {
                            let runtime_api = client.runtime_api();
                            let block_hash = client.usage_info().chain.best_hash;
                            match runtime_api.execute_task(block_hash, func_ref, payload) {
                                Ok(payload) => {
                                    rx.send(payload).unwrap();
                                }
                                Err(e) => {
                                    log::error!("`GearTasksApi::execute_task` failed: {e}");
                                }
                            }
                        });
                    }
                    TaskEvent::SpawnNative {
                        func_ref,
                        payload,
                        rx,
                    } => {
                        self.thread_pool.spawn_ok(async move {
                            let payload = (func_ref)(payload);
                            rx.send(payload).unwrap();
                        });
                    }
                }
            }
        }
    }

    sp_externalities::decl_extension! {
        #[derive(Default)]
        pub struct TaskSpawnerExt(TaskSpawner);
    }

    #[derive(Default)]
    pub struct TaskSpawner {
        counter: u64,
        tasks: HashMap<u64, mpsc::Receiver<Vec<u8>>>,
    }

    impl TaskSpawner {
        fn spawn_inner(
            &mut self,
            build_event: impl FnOnce(mpsc::SyncSender<Vec<u8>>) -> TaskEvent,
        ) -> JoinHandle {
            let handle = self.counter;
            self.counter += 1;

            let (rx, tx) = mpsc::sync_channel(1);

            let spawner_tx = TX.get().expect("`GearTasksRunner` is not spawned");
            spawner_tx.send(build_event(rx)).unwrap();

            self.tasks.insert(handle, tx);
            JoinHandle { inner: handle }
        }

        pub(crate) fn spawn_wasm(&mut self, func_ref: u64, payload: Vec<u8>) -> JoinHandle {
            self.spawn_inner(move |rx| TaskEvent::SpawnWasm {
                func_ref,
                payload,
                rx,
            })
        }

        pub(crate) fn spawn_native(
            &mut self,
            func_ref: fn(Vec<u8>) -> Vec<u8>,
            payload: Vec<u8>,
        ) -> JoinHandle {
            self.spawn_inner(move |rx| TaskEvent::SpawnNative {
                func_ref,
                payload,
                rx,
            })
        }

        pub(crate) fn join(&mut self, handle: JoinHandle) -> Vec<u8> {
            let tx = self
                .tasks
                .remove(&handle.inner)
                .expect("`JoinHandle` is duplicated so task not found");
            tx.recv()
                .expect("Sender has been disconnected which means thread was somehow terminated")
        }
    }

    #[derive(Debug, Eq, PartialEq)]
    pub struct JoinHandle {
        pub(super) inner: u64,
    }

    impl JoinHandle {
        pub fn join(self) -> Vec<u8> {
            sp_externalities::with_externalities(|mut ext| {
                let spawner = ext
                    .extension::<TaskSpawnerExt>()
                    .expect("Cannot join without dynamic runtime dispatcher (TaskSpawnerExt)");
                spawner.join(self)
            })
            .expect("`spawn`: called outside of externalities context")
        }
    }

    pub fn spawn(f: fn(Vec<u8>) -> Vec<u8>, payload: Vec<u8>) -> JoinHandle {
        sp_externalities::with_externalities(|mut ext| {
            let spawner = ext
                .extension::<TaskSpawnerExt>()
                .expect("Cannot join without dynamic runtime dispatcher (TaskSpawnerExt)");
            spawner.spawn_native(f, payload)
        })
        .expect("`spawn`: called outside of externalities context")
    }

    #[cfg(test)]
    mod tests {
        use super::*;
        use crate::inner::TaskSpawnerExt;
        use gear_node_testing::client::{
            Client as TestClient, TestClientBuilder, TestClientBuilderExt,
        };
        use std::sync::Arc;

        fn init_logger() {
            let _ = env_logger::Builder::from_default_env()
                .format_module_path(false)
                .format_level(true)
                .try_init();
        }

        fn new_test_ext() -> sp_io::TestExternalities {
            let client: Arc<TestClient> = Arc::new(TestClientBuilder::new().build());
            let runner = GearTasksRunner::new(client);

            std::thread::spawn(|| {
                futures_executor::block_on(async move {
                    runner.run().await;
                });
            });

            let mut ext = sp_io::TestExternalities::new_empty();
            ext.register_extension(TaskSpawnerExt::default());
            ext
        }

        #[test]
        fn smoke_native() {
            init_logger();
            new_test_ext().execute_with(|| {
                const PAYLOAD_SIZE: usize = 32 * 1024 * 1024;

                let payload = vec![0xff; PAYLOAD_SIZE];
                let handles = (0..TASKS_AMOUNT).map(|i| {
                    let mut payload = payload.clone();
                    payload[i * (PAYLOAD_SIZE / TASKS_AMOUNT)] = 0xfe;
                    spawn(
                        |mut payload| {
                            payload.sort();
                            payload
                        },
                        payload,
                    )
                });

                let mut expected = vec![0xff; PAYLOAD_SIZE];
                expected[0] = 0xfe;

                for handle in handles {
                    let payload = handle.join();
                    assert_eq!(payload, expected);
                }
            })
        }
    }
}

#[cfg(not(feature = "std"))]
#[cfg(target_arch = "wasm32")]
mod inner {
    #[derive(Debug, Eq, PartialEq)]
    pub struct JoinHandle {
        pub(crate) inner: u64,
    }

    impl JoinHandle {
        pub fn join(self) -> Vec<u8> {
            gear_tasks::join(self.inner)
        }
    }

    pub fn spawn(f: fn(Vec<u8>) -> Vec<u8>, payload: Vec<u8>) -> JoinHandle {
        let inner = gear_tasks::spawn(f as usize as u64, payload);
        JoinHandle { inner }
    }
}
