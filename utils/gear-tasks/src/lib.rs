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

use futures_executor::ThreadPool;
use sc_executor::GearVersionedRuntimeExt;
use sc_executor_common::wasm_runtime::InvokeMethod;
use sp_externalities::ExternalitiesExt;
use sp_runtime_interface::runtime_interface;
use std::{
    collections::HashMap,
    sync::{
        atomic::{AtomicU64, Ordering},
        mpsc,
    },
};

const TASKS_AMOUNT: usize = 4;

sp_externalities::decl_extension! {
    struct TaskSpawnerExt(TaskSpawner);
}

struct TaskSpawner {
    thread_pool: ThreadPool,
    handle_counter: AtomicU64,
    tasks: HashMap<u64, mpsc::Receiver<Vec<u8>>>,
}

impl TaskSpawner {
    fn new() -> Self {
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

    fn spawn_via_dispatcher(
        &mut self,
        dispatcher_ref: u32,
        entry: u32,
        payload: Vec<u8>,
    ) -> JoinHandle {
        self.spawn_inner(
            move |payload| {
                sp_externalities::with_externalities(|mut ext| {
                    let runtime = ext
                        .extension::<GearVersionedRuntimeExt>()
                        .expect("`GearVersionedRuntimeExt` is not set")
                        .clone();
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
                .expect(
                    "`TaskSpawner::spawn_via_dispatcher`: called outside of externalities context",
                )
            },
            payload,
        )
    }

    fn join(&mut self, handle: JoinHandle) -> Vec<u8> {
        let tx = self
            .tasks
            .remove(&handle.inner)
            .expect("`JoinHandle` is duplicated so task not found");
        tx.recv()
            .expect("Sender has been disconnected which means thread was somehow terminated")
    }
}

/// WASM host functions for managing tasks.
#[runtime_interface(wasm_only)]
trait RuntimeTasks {
    fn spawn(dispatcher_ref: u32, entry: u32, payload: Vec<u8>) -> u64 {
        sp_externalities::with_externalities(|mut ext| {
            let spawner = ext
                .extension::<TaskSpawnerExt>()
                .expect("Cannot spawn without dynamic runtime dispatcher (TaskSpawnerExt)");
            let handle = spawner.spawn_via_dispatcher(dispatcher_ref, entry, payload);
            handle.inner
        })
        .expect("`RuntimeTasks::spawn`: called outside of externalities context")
    }

    fn join(handle: u64) -> Vec<u8> {
        sp_externalities::with_externalities(|mut ext| {
            let spawner = ext
                .extension::<TaskSpawnerExt>()
                .expect("Cannot join without dynamic runtime dispatcher (TaskSpawnerExt)");
            spawner.join(JoinHandle { inner: handle })
        })
        .expect("`RuntimeTasks::join`: called outside of externalities context")
    }
}

pub use inner::{spawn, JoinHandle};

#[cfg(feature = "std")]
mod inner {
    use super::*;

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
}

#[cfg(not(feature = "std"))]
#[cfg(target_arch = "wasm32")]
mod inner {
    use super::*;

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
            runtime_tasks::join(self.inner)
        }
    }

    pub fn spawn(f: fn(Vec<u8>) -> Vec<u8>, payload: Vec<u8>) -> JoinHandle {
        let func_ref = f as *const fn(Vec<u8>) -> Vec<u8> as *const u8;
        let handle = runtime_tasks::spawn(dispatch_wrapper as usize as u32, func_ref, payload);
        JoinHandle { inner: handle }
    }
}
