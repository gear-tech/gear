// This file is part of Gear.

// Copyright (C) 2021-2024 Gear Technologies Inc.
// SPDX-License-Identifier: GPL-3.0-or-later WITH Classpath-exception-2.0

// This program is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.

// This program is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE. See the
// GNU General Public License for more details.

// You should have received a copy of the GNU General Public License
// along with this program. If not, see <https://www.gnu.org/licenses/>.

//! Utilities to build a `TestClient` for gear- or vara-runtime.

use sp_runtime::BuildStorage;
/// Re-export test-client utilities.
pub use substrate_test_client::*;
use vara_runtime as runtime;

// A unit struct which implements `NativeExecutionDispatch` feeding in the hard-coded runtime
pub struct LocalExecutorDispatch;

impl sc_executor::NativeExecutionDispatch for LocalExecutorDispatch {
    type ExtendHostFunctions = (
        frame_benchmarking::benchmarking::HostFunctions,
        gear_runtime_interface::gear_ri::HostFunctions,
        gear_runtime_interface::sandbox::HostFunctions,
        gear_runtime_interface::gear_tasks::HostFunctions,
        sp_crypto_ec_utils::bls12_381::host_calls::HostFunctions,
        gear_runtime_interface::gear_bls_12_381::HostFunctions,
    );

    fn dispatch(method: &str, data: &[u8]) -> Option<Vec<u8>> {
        runtime::api::dispatch(method, data)
    }

    fn native_version() -> sc_executor::NativeVersion {
        runtime::native_version()
    }
}

pub type ExecutorDispatch = sc_executor::NativeElseWasmExecutor<LocalExecutorDispatch>;

/// Test client backend.
pub type Backend = substrate_test_client::Backend<runtime_primitives::Block>;

/// Test client type.
pub type Client = client::Client<
    Backend,
    client::LocalCallExecutor<runtime_primitives::Block, Backend, ExecutorDispatch>,
    runtime_primitives::Block,
    runtime::RuntimeApi,
>;

/// Genesis configuration parameters for `TestClient`.
#[derive(Default)]
pub struct GenesisParameters;

impl substrate_test_client::GenesisInit for GenesisParameters {
    fn genesis_storage(&self) -> Storage {
        crate::genesis::genesis_config(None)
            .build_storage()
            .unwrap()
    }
}

/// A `test-runtime` extensions to `TestClientBuilder`.
pub trait TestClientBuilderExt: Sized {
    /// Create test client builder.
    fn new() -> Self;

    /// Build the test client.
    fn build(self) -> Client;

    /// Build the test client with customized executor.
    fn build_with_wasm_executor(self, executor: Option<ExecutorDispatch>) -> Client;
}

impl TestClientBuilderExt
    for substrate_test_client::TestClientBuilder<
        runtime_primitives::Block,
        client::LocalCallExecutor<runtime_primitives::Block, Backend, ExecutorDispatch>,
        Backend,
        GenesisParameters,
    >
{
    fn new() -> Self {
        Self::default()
    }

    fn build(self) -> Client {
        self.build_with_native_executor(None).0
    }

    fn build_with_wasm_executor(self, executor: Option<ExecutorDispatch>) -> Client {
        let executor = executor.unwrap_or_else(|| {
            NativeElseWasmExecutor::new_with_wasm_executor(WasmExecutor::builder().build())
        });

        self.build_with_native_executor(executor).0
    }
}
