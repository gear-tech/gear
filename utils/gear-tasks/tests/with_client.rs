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

use gear_node_testing::client::{
    Client as TestClient, NativeElseWasmExecutor, TestClientBuilder, TestClientBuilderExt,
    WasmExecutor,
};
use gear_tasks::GearTasksRunner;
use sc_client_api::UsageProvider;
use sp_api::HashingFor;
use sp_externalities::Extensions;
use sp_state_machine::{Ext, OverlayedChanges};
use std::{
    sync::{Arc, OnceLock},
    time::Duration,
};
use vara_runtime::{Block, ProcessingTasksAmount};

static CLIENT: OnceLock<Arc<TestClient>> = OnceLock::new();

struct BackendExternalities {
    extensions: Extensions,
    overlay: OverlayedChanges<HashingFor<Block>>,
}

impl Default for BackendExternalities {
    fn default() -> Self {
        let mut overlay = OverlayedChanges::default();
        // emulate actual runtime behavior
        overlay.enter_runtime().unwrap();

        Self {
            extensions: Default::default(),
            overlay,
        }
    }
}

impl BackendExternalities {
    fn execute_with<R>(&mut self, f: impl FnOnce() -> R) -> R {
        let client = CLIENT.get().unwrap();
        let block_hash = client.usage_info().chain.best_hash;
        let state = client.state_at(block_hash).unwrap();

        let mut ext = Ext::new(&mut self.overlay, &state, Some(&mut self.extensions));
        sp_externalities::set_and_run_with_externalities(&mut ext, || {
            gear_tasks::gear_tasks::reinit(ProcessingTasksAmount::get());
            f()
        })
    }
}

pub fn init_logger() {
    let _ = env_logger::Builder::from_default_env()
        .format(|f, record| {
            use std::io::Write;

            let current_thread = std::thread::current();

            let level = f.default_styled_level(record.level());
            let module = record.module_path().unwrap_or_default();
            let thread_name = current_thread
                .name()
                .map(str::to_string)
                .unwrap_or_else(|| format!("{:?}", current_thread.id()));

            writeln!(f, "[{level:<5} ({thread_name}) {module}] {}", record.args(),)
        })
        .try_init();
}

fn new_test_ext() -> BackendExternalities {
    CLIENT.get_or_init(|| {
        let mut executor =
            NativeElseWasmExecutor::new_with_wasm_executor(WasmExecutor::builder().build());
        // Substrate's `CodeExecutor::call()` has explicit flag to use native execution,
        // so it's applicable for `NativeElseWasmExecutor`, too.
        // The flag is always set to `false` in our case, so
        // we set it to true
        executor.gear_force_native();
        let client = TestClientBuilder::new().build_with_wasm_executor(Some(executor));

        let client = Arc::new(client);

        let runner = GearTasksRunner::new(client.clone());

        std::thread::spawn(|| {
            futures_executor::block_on(async move {
                runner.run().await;
            });
        });

        client
    });

    BackendExternalities::default()
}

#[test]
fn smoke_native() {
    init_logger();
    new_test_ext().execute_with(|| {
        const PAYLOAD_SIZE: usize = 32 * 1024;

        let payload = vec![0xff; PAYLOAD_SIZE];
        let handles = (0..ProcessingTasksAmount::get() as usize).map(|i| {
            let mut payload = payload.clone();
            payload[i * (PAYLOAD_SIZE / ProcessingTasksAmount::get() as usize)] = 0xfe;
            gear_tasks::spawn(
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
            let payload = handle.join().unwrap();
            assert_eq!(payload, expected);
        }
    })
}

#[test]
fn write_has_no_effect() {
    init_logger();
    new_test_ext().execute_with(|| {
        const MAIN_KEY: &[u8] = b"MAIN_KEY";
        const MAIN_VALUE: &[u8] = b"MAIN_VALUE";

        const THREAD_KEY: &[u8] = b"THREAD_KEY";
        const THREAD_VALUE: &[u8] = b"THREAD_VALUE";

        sp_io::storage::set(MAIN_KEY, MAIN_VALUE);
        assert_eq!(sp_io::storage::get(MAIN_KEY).as_deref(), Some(MAIN_VALUE));

        gear_tasks::spawn(
            |_payload| {
                assert_eq!(sp_io::storage::get(MAIN_KEY).as_deref(), Some(MAIN_VALUE));

                sp_io::storage::set(THREAD_KEY, THREAD_VALUE);

                vec![]
            },
            vec![],
        )
        .join()
        .unwrap();

        gear_tasks::spawn(
            |_payload| {
                assert_eq!(sp_io::storage::get(MAIN_KEY).as_deref(), Some(MAIN_VALUE));

                assert_eq!(sp_io::storage::get(THREAD_KEY), None);
                vec![]
            },
            vec![],
        )
        .join()
        .unwrap();

        assert_eq!(sp_io::storage::get(MAIN_KEY).as_deref(), Some(MAIN_VALUE));
        assert_eq!(sp_io::storage::get(THREAD_KEY), None);
    });
}

#[test]
#[should_panic = "Not every task has been joined"]
fn unjoined_task_detected() {
    init_logger();
    new_test_ext().execute_with(|| {
        let _handle = gear_tasks::spawn(
            |_payload| {
                std::thread::sleep(Duration::MAX);
                vec![]
            },
            vec![],
        );
    });
}
