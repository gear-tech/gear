// This file is part of Gear.

// Copyright (C) 2021-2022 Gear Technologies Inc.
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

//! Gear test client

#![allow(missing_docs)]

pub mod trait_tests;

mod block_builder_ext;

pub use sc_consensus::LongestChain;
use std::sync::Arc;
pub use substrate_test_client::*;
pub use test_runtime as runtime;

pub use self::block_builder_ext::BlockBuilderExt;

use sp_core::{
    sr25519,
    storage::{ChildInfo, Storage, StorageChild},
    Pair,
};
use sp_runtime::traits::{Block as BlockT, Hash as HashT, Header as HeaderT};
use test_runtime::genesismap::{additional_storage_with_genesis, GenesisConfig};

// A prelude to import in tests.
pub mod prelude {
    // Trait extensions
    pub use super::{
        BlockBuilderExt, ClientBlockImportExt, ClientExt, DefaultTestClientBuilderExt,
        TestClientBuilderExt,
    };
    // Client structs
    pub use super::{
        Backend, ExecutorDispatch, LocalExecutorDispatch, NativeElseWasmExecutor, TestClient,
        TestClientBuilder, WasmExecutionMethod,
    };
    // Keyring
    pub use super::{AccountKeyring, Sr25519Keyring};
}

// A unit struct which implements `NativeExecutionDispatch` feeding in the hard-coded runtime
pub struct LocalExecutorDispatch;

impl sc_executor::NativeExecutionDispatch for LocalExecutorDispatch {
    type ExtendHostFunctions = ();

    fn dispatch(method: &str, data: &[u8]) -> Option<Vec<u8>> {
        runtime::api::dispatch(method, data)
    }

    fn native_version() -> sc_executor::NativeVersion {
        runtime::native_version()
    }
}

// Test client database backend
pub type Backend = substrate_test_client::Backend<runtime::Block>;

// Test client executor
pub type ExecutorDispatch = client::LocalCallExecutor<
    runtime::Block,
    Backend,
    NativeElseWasmExecutor<LocalExecutorDispatch>,
>;

// Parameters of test-client builder with test-runtime
#[derive(Default)]
pub struct GenesisParameters {
    heap_pages_override: Option<u64>,
    extra_storage: Storage,
    wasm_code: Option<Vec<u8>>,
}

impl GenesisParameters {
    fn genesis_config(&self) -> GenesisConfig {
        GenesisConfig::new(
            vec![
                sr25519::Public::from(Sr25519Keyring::Alice).into(),
                sr25519::Public::from(Sr25519Keyring::Bob).into(),
                sr25519::Public::from(Sr25519Keyring::Charlie).into(),
            ],
            (0..16_usize)
                .into_iter()
                .map(|i| AccountKeyring::numeric(i).public())
                .chain(vec![
                    AccountKeyring::Alice.into(),
                    AccountKeyring::Bob.into(),
                    AccountKeyring::Charlie.into(),
                ])
                .collect(),
            1000,
            self.heap_pages_override,
            self.extra_storage.clone(),
        )
    }

    // Set the wasm code that should be used at genesis.
    pub fn set_wasm_code(&mut self, code: Vec<u8>) {
        self.wasm_code = Some(code);
    }

    // Access extra genesis storage.
    pub fn extra_storage(&mut self) -> &mut Storage {
        &mut self.extra_storage
    }
}

impl substrate_test_client::GenesisInit for GenesisParameters {
    fn genesis_storage(&self) -> Storage {
        use codec::Encode;

        let mut storage = self.genesis_config().genesis_map();

        if let Some(ref code) = self.wasm_code {
            storage.top.insert(
                sp_core::storage::well_known_keys::CODE.to_vec(),
                code.clone(),
            );
        }

        let child_roots = storage.children_default.iter().map(|(_sk, child_content)| {
            let state_root =
                <<<runtime::Block as BlockT>::Header as HeaderT>::Hashing as HashT>::trie_root(
                    child_content.data.clone().into_iter().collect(),
                    sp_runtime::StateVersion::V1,
                );
            let prefixed_storage_key = child_content.child_info.prefixed_storage_key();
            (prefixed_storage_key.into_inner(), state_root.encode())
        });
        let state_root =
            <<<runtime::Block as BlockT>::Header as HeaderT>::Hashing as HashT>::trie_root(
                storage.top.clone().into_iter().chain(child_roots).collect(),
                sp_runtime::StateVersion::V1,
            );
        let block: runtime::Block = client::genesis::construct_genesis_block(state_root);
        storage.top.extend(additional_storage_with_genesis(&block));

        storage
    }
}

// A `TestClient` with `test-runtime` builder.
pub type TestClientBuilder<E, B> =
    substrate_test_client::TestClientBuilder<runtime::Block, E, B, GenesisParameters>;

// Test client type with `LocalExecutorDispatch` and generic Backend.
pub type Client<B> = client::Client<
    B,
    client::LocalCallExecutor<
        runtime::Block,
        B,
        sc_executor::NativeElseWasmExecutor<LocalExecutorDispatch>,
    >,
    runtime::Block,
    runtime::RuntimeApi,
>;

// A test client with default backend.
pub type TestClient = Client<Backend>;

// A `TestClientBuilder` with default backend and executor.
pub trait DefaultTestClientBuilderExt: Sized {
    // Create new `TestClientBuilder`
    fn new() -> Self;
}

impl DefaultTestClientBuilderExt for TestClientBuilder<ExecutorDispatch, Backend> {
    fn new() -> Self {
        Self::with_default_backend()
    }
}

// A `test-runtime` extensions to `TestClientBuilder`.
pub trait TestClientBuilderExt<B>: Sized {
    // Returns a mutable reference to the genesis parameters.
    fn genesis_init_mut(&mut self) -> &mut GenesisParameters;

    // Override the default value for Wasm heap pages.
    fn set_heap_pages(mut self, heap_pages: u64) -> Self {
        self.genesis_init_mut().heap_pages_override = Some(heap_pages);
        self
    }

    // Add an extra value into the genesis storage.
    //
    // # Panics
    //
    // Panics if the key is empty.
    fn add_extra_child_storage<K: Into<Vec<u8>>, V: Into<Vec<u8>>>(
        mut self,
        child_info: &ChildInfo,
        key: K,
        value: V,
    ) -> Self {
        let storage_key = child_info.storage_key().to_vec();
        let key = key.into();
        assert!(!storage_key.is_empty());
        assert!(!key.is_empty());
        self.genesis_init_mut()
            .extra_storage
            .children_default
            .entry(storage_key)
            .or_insert_with(|| StorageChild {
                data: Default::default(),
                child_info: child_info.clone(),
            })
            .data
            .insert(key, value.into());
        self
    }

    // Add an extra child value into the genesis storage.
    //
    // # Panics
    //
    // Panics if the key is empty.
    fn add_extra_storage<K: Into<Vec<u8>>, V: Into<Vec<u8>>>(mut self, key: K, value: V) -> Self {
        let key = key.into();
        assert!(!key.is_empty());
        self.genesis_init_mut()
            .extra_storage
            .top
            .insert(key, value.into());
        self
    }

    // Build the test client.
    fn build(self) -> Client<B> {
        self.build_with_longest_chain().0
    }

    // Build the test client and longest chain selector.
    fn build_with_longest_chain(self)
        -> (Client<B>, sc_consensus::LongestChain<B, runtime::Block>);

    // Build the test client and the backend.
    fn build_with_backend(self) -> (Client<B>, Arc<B>);
}

impl<B> TestClientBuilderExt<B>
    for TestClientBuilder<
        client::LocalCallExecutor<
            runtime::Block,
            B,
            sc_executor::NativeElseWasmExecutor<LocalExecutorDispatch>,
        >,
        B,
    >
where
    B: sc_client_api::backend::Backend<runtime::Block> + 'static,
{
    fn genesis_init_mut(&mut self) -> &mut GenesisParameters {
        Self::genesis_init_mut(self)
    }

    fn build_with_longest_chain(
        self,
    ) -> (Client<B>, sc_consensus::LongestChain<B, runtime::Block>) {
        self.build_with_native_executor(None)
    }

    fn build_with_backend(self) -> (Client<B>, Arc<B>) {
        let backend = self.backend();
        (self.build_with_native_executor(None).0, backend)
    }
}

// Creates new client instance used for tests.
pub fn new() -> Client<Backend> {
    TestClientBuilder::new().build()
}

// Create a new native executor.
pub fn new_native_executor() -> sc_executor::NativeElseWasmExecutor<LocalExecutorDispatch> {
    sc_executor::NativeElseWasmExecutor::new(
        sc_executor::WasmExecutionMethod::Interpreted,
        None,
        8,
        2,
    )
}
