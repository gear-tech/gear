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
    Backend as TestBackend, Client as TestClient, TestClientBuilder, TestClientBuilderExt,
};
use gear_tasks::{GearTasksRunner, TaskSpawnerExt, TASKS_AMOUNT};
use sc_client_api::{Backend, UsageProvider};
use sp_externalities::{Extension, Extensions, ExternalitiesExt};
use sp_state_machine::{Ext, OverlayedChanges};
use sp_storage::StateVersion;
use std::sync::{Arc, OnceLock};

static BACKEND: OnceLock<Arc<TestBackend>> = OnceLock::new();
static CLIENT: OnceLock<Arc<TestClient>> = OnceLock::new();

#[derive(Default)]
struct BackendExternalities {
    extensions: Extensions,
}

impl BackendExternalities {
    fn register_extension<T: Extension>(&mut self, ext: T) {
        self.extensions.register(ext);
    }

    fn execute_with<R>(&mut self, f: impl FnOnce() -> R) -> R {
        let client = CLIENT.get().unwrap();
        let block_hash = client.usage_info().chain.best_hash;

        let mut overlay = OverlayedChanges::default();
        let state = BACKEND.get().unwrap().state_at(block_hash).unwrap();
        let mut ext = Ext::new(&mut overlay, &state, Some(&mut self.extensions));
        sp_externalities::set_and_run_with_externalities(&mut ext, f)
    }
}

pub fn init_logger() {
    let _ = env_logger::Builder::from_default_env()
        .format(|f, record| {
            use std::io::Write;

            let current_thread = std::thread::current();
            writeln!(
                f,
                "[{} {}] |{}| {}",
                record.level(),
                record.module_path().unwrap_or_default(),
                current_thread
                    .name()
                    .map(str::to_string)
                    .unwrap_or_else(|| { format!("{:?}", current_thread.id()) }),
                record.args(),
            )
        })
        .try_init();
}

pub fn new_test_ext() -> BackendExternalities {
    CLIENT.get_or_init(|| {
        let backend = BACKEND.get_or_init(|| Arc::new(TestBackend::new_test(u32::MAX, u64::MAX)));
        let builder = TestClientBuilder::with_backend(backend.clone());
        let mut client = builder.build();
        // Substrate's `CodeExecutor::call()` has explicit flag to use native execution,
        // so it's applicable for `NativeElseWasmExecutor`, too.
        // The flag is always set to `false` in our case, so
        // we set it to true
        client.gear_use_native();
        let client = Arc::new(client);

        let runner = GearTasksRunner::new(client.clone());

        std::thread::spawn(|| {
            futures_executor::block_on(async move {
                runner.run().await;
            });
        });

        client
    });

    sp_io::TestExternalities::default();

    let mut ext = BackendExternalities::default();
    ext.register_extension(TaskSpawnerExt::default());
    ext
}

#[test]
fn smoke_native() {
    init_logger();
    new_test_ext().execute_with(|| {
        const PAYLOAD_SIZE: usize = 32 * 1024;

        let payload = vec![0xff; PAYLOAD_SIZE];
        let handles = (0..TASKS_AMOUNT).map(|i| {
            let mut payload = payload.clone();
            payload[i * (PAYLOAD_SIZE / TASKS_AMOUNT)] = 0xfe;
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
//#[should_panic = r#"RuntimeApi("Execution failed: Runtime panicked: assertion `left == right` failed"#]
fn read_denied() {
    init_logger();
    new_test_ext().execute_with(|| {
        const GLOBAL_AVAILABLE_KEY: &[u8] = b"GLOBAL_AVAILABLE_KEY";
        const GLOBAL_AVAILABLE_VALUE: &[u8] = b"GLOBAL_AVAILABLE_VALUE";

        sp_io::storage::set(GLOBAL_AVAILABLE_KEY, GLOBAL_AVAILABLE_VALUE);
        assert_eq!(
            sp_io::storage::get(GLOBAL_AVAILABLE_KEY).as_deref(),
            Some(GLOBAL_AVAILABLE_VALUE)
        );

        assert_eq!(
            sp_io::storage::get(GLOBAL_AVAILABLE_KEY).as_deref(),
            Some(GLOBAL_AVAILABLE_VALUE)
        );

        gear_tasks::spawn(
            |_payload| {
                assert_eq!(
                    sp_io::storage::get(GLOBAL_AVAILABLE_KEY).as_deref(),
                    Some(GLOBAL_AVAILABLE_VALUE)
                );

                sp_io::storage::set(b"SOME_NEW_KEY", b"SOME_NEW_VALUE");
                vec![]
            },
            vec![],
        )
        .join()
        .unwrap();

        gear_tasks::spawn(
            |_payload| {
                assert_eq!(
                    sp_io::storage::get(GLOBAL_AVAILABLE_KEY).as_deref(),
                    Some(GLOBAL_AVAILABLE_VALUE)
                );

                assert_eq!(sp_io::storage::get(b"SOME_NEW_KEY"), None);
                vec![]
            },
            vec![],
        )
        .join()
        .unwrap();

        assert_eq!(
            sp_io::storage::get(GLOBAL_AVAILABLE_KEY).as_deref(),
            Some(GLOBAL_AVAILABLE_VALUE)
        );
        assert_eq!(sp_io::storage::get(b"SOME_NEW_KEY"), None);
    });
}
