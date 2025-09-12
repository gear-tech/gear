// This file is part of Gear.

// Copyright (C) 2021-2025 Gear Technologies Inc.
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

pub use service::RuntimeExecutor;
use sp_runtime::BuildStorage;
/// Re-export test-client utilities.
pub use substrate_test_client::*;
use vara_runtime as runtime;

pub type ExtendHostFunctions = (
    gear_runtime_interface::gear_ri::HostFunctions,
    gear_runtime_interface::sandbox::HostFunctions,
    sp_crypto_ec_utils::bls12_381::host_calls::HostFunctions,
    gear_runtime_interface::gear_bls_12_381::HostFunctions,
    gear_runtime_interface::gear_webpki::HostFunctions,
);

/// Test client backend.
pub type Backend = substrate_test_client::Backend<runtime_primitives::Block>;

/// Test client type.
pub type Client = client::Client<
    Backend,
    client::LocalCallExecutor<runtime_primitives::Block, Backend, RuntimeExecutor>,
    runtime_primitives::Block,
    runtime::RuntimeApi,
>;

/// Genesis configuration parameters for `TestClient`.
#[derive(Default)]
pub struct GenesisParameters;

impl substrate_test_client::GenesisInit for GenesisParameters {
    fn genesis_storage(&self) -> Storage {
        let mut storage = crate::genesis::genesis_config().build_storage().unwrap();
        storage.top.insert(
			sp_core::storage::well_known_keys::CODE.to_vec(),
			vara_runtime::WASM_BINARY.expect(
                "Development wasm is not available. Rebuild with the `SKIP_WASM_BUILD` flag disabled.",
            ).into(),
		);
        storage
    }
}

/// A `test-runtime` extensions to `TestClientBuilder`.
pub trait TestClientBuilderExt: Sized {
    /// Create test client builder.
    fn new() -> Self;

    /// Build the test client.
    fn build(self, executor: Option<RuntimeExecutor>) -> Client;
}

impl TestClientBuilderExt
    for substrate_test_client::TestClientBuilder<
        runtime_primitives::Block,
        client::LocalCallExecutor<runtime_primitives::Block, Backend, RuntimeExecutor>,
        Backend,
        GenesisParameters,
    >
{
    fn new() -> Self {
        Self::default()
    }
    fn build(self, executor: Option<RuntimeExecutor>) -> Client {
        let executor = executor.unwrap_or_else(|| RuntimeExecutor::builder().build());
        use sc_service::client::LocalCallExecutor;
        use std::sync::Arc;
        let executor = LocalCallExecutor::new(
            self.backend().clone(),
            executor.clone(),
            Default::default(),
            ExecutionExtensions::new(None, Arc::new(executor)),
        )
        .expect("Creates LocalCallExecutor");
        self.build_with_executor(executor).0
    }
}
