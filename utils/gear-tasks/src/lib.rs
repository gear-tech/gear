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

pub use inner::{spawn, JoinHandle};

use alloc::vec::Vec;
use sc_executor::GearVersionedRuntimeExt;
use sp_externalities::ExternalitiesExt;
use sp_runtime_interface::runtime_interface;

/// WASM host functions for managing tasks.
#[runtime_interface]
pub trait GearTasks {
    fn init(&mut self) {
        self.register_extension(inner::TaskSpawnerExt::default())
            .expect("`GearTasks` initialized twice");
    }

    fn spawn(&mut self, dispatcher_ref: u32, entry: u32, payload: Vec<u8>) -> u64 {
        let runtime = self
            .extension::<GearVersionedRuntimeExt>()
            .expect("`GearVersionedRuntimeExt` is not set")
            .clone();

        let spawner = self
            .extension::<inner::TaskSpawnerExt>()
            .expect("Cannot spawn without dynamic runtime dispatcher (TaskSpawnerExt)");
        let handle = spawner.spawn_via_dispatcher(runtime, dispatcher_ref, entry, payload);
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
    use sc_executor::VersionedRuntime;
    use sc_executor_common::wasm_runtime::InvokeMethod;
    use sp_externalities::{Error, Extension, ExtensionStore, Externalities, MultiRemovalResults};
    use std::{
        any::{Any, TypeId},
        collections::HashMap,
        sync::{
            atomic::{AtomicU64, Ordering},
            mpsc, Arc,
        },
    };

    const TASKS_AMOUNT: usize = 4;

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

    sp_externalities::decl_extension! {
        #[derive(Default)]
        pub struct TaskSpawnerExt(TaskSpawner);
    }

    pub struct TaskSpawner {
        thread_pool: ThreadPool,
        handle_counter: AtomicU64,
        tasks: HashMap<u64, mpsc::Receiver<Vec<u8>>>,
    }

    impl Default for TaskSpawner {
        fn default() -> Self {
            Self {
                thread_pool: ThreadPool::builder()
                    .pool_size(TASKS_AMOUNT)
                    .name_prefix("gear-tasks-")
                    .create()
                    .expect("Thread pool creation failed"),
                handle_counter: AtomicU64::new(0),
                tasks: HashMap::new(),
            }
        }
    }

    impl TaskSpawner {
        fn spawn_inner(
            &mut self,
            f: impl FnOnce(Vec<u8>) -> Vec<u8> + Send + 'static,
            payload: Vec<u8>,
        ) -> JoinHandle {
            let handle = self.handle_counter.fetch_add(1, Ordering::Relaxed);
            let (rx, tx) = mpsc::sync_channel(1);
            self.thread_pool.spawn_ok(async move {
                if let Err(_e) = rx.send(f(payload)) {
                    log::debug!("Receiver has been disconnected for {handle}");
                }
            });
            self.tasks.insert(handle, tx);
            JoinHandle { inner: handle }
        }

        fn spawn(&mut self, f: fn(Vec<u8>) -> Vec<u8>, payload: Vec<u8>) -> JoinHandle {
            self.spawn_inner(f, payload)
        }

        pub(crate) fn spawn_via_dispatcher(
            &mut self,
            runtime: Arc<VersionedRuntime>,
            dispatcher_ref: u32,
            entry: u32,
            payload: Vec<u8>,
        ) -> JoinHandle {
            self.spawn_inner(
                move |payload| {
                    sp_externalities::set_and_run_with_externalities(
                        &mut NoStorageExternalities,
                        || {
                            sp_externalities::with_externalities(|ext| {
                                runtime
                                    .with_instance(ext, |_module, instance, _version, _ext| {
                                        let payload = instance
                                            .call(
                                                InvokeMethod::TableWithWrapper {
                                                    dispatcher_ref,
                                                    func: entry,
                                                },
                                                &payload,
                                            )
                                            .expect("WASM execution failed");
                                        Ok(payload)
                                    })
                                    .expect("Instantiation failed")
                            })
                            .expect("Externalities are set above; qed")
                        },
                    )
                },
                payload,
            )
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
            spawner.spawn(f, payload)
        })
        .expect("`spawn`: called outside of externalities context")
    }

    #[cfg(test)]
    mod tests {
        use super::*;

        fn new_test_ext() -> sp_io::TestExternalities {
            let mut ext = sp_io::TestExternalities::new_empty();

            ext.register_extension(TaskSpawnerExt::default());

            ext
        }

        #[test]
        fn smoke() {
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
    use super::*;

    use core::mem;

    /// Dispatch wrapper for WASM blob.
    ///
    /// Serves as trampoline to call any rust function with (Vec<u8>) -> Vec<u8> compiled
    /// into the runtime.
    ///
    /// Function item should be provided with `func_ref`. Argument for the call
    /// will be generated from bytes at `payload_ptr` with `payload_len`.
    ///
    /// NOTE: Since this dynamic dispatch function and the invoked function are compiled with
    /// the same compiler, there should be no problem with ABI incompatibility.
    extern "C" fn dispatch_wrapper(
        func_ref: *const u8,
        payload_ptr: *mut u8,
        payload_len: u32,
    ) -> u64 {
        let payload_len = payload_len as usize;
        let output = unsafe {
            let payload = Vec::from_raw_parts(payload_ptr, payload_len, payload_len);
            let ptr: fn(Vec<u8>) -> Vec<u8> = mem::transmute(func_ref);
            (ptr)(payload)
        };
        sp_runtime_interface::pack_ptr_and_len(output.as_ptr() as usize as _, output.len() as _)
    }

    #[derive(Debug, Eq, PartialEq)]
    pub struct JoinHandle {
        pub(super) inner: u64,
    }

    impl JoinHandle {
        pub fn join(self) -> Vec<u8> {
            gear_tasks::join(self.inner)
        }
    }

    pub fn spawn(f: fn(Vec<u8>) -> Vec<u8>, payload: Vec<u8>) -> JoinHandle {
        let func_ref = f as usize as u32;
        let handle = gear_tasks::spawn(dispatch_wrapper as usize as u32, func_ref, payload);
        JoinHandle { inner: handle }
    }
}
