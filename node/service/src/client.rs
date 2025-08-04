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

use gear_runtime_interface as gear_ri;
pub use runtime_primitives::{AccountId, Balance, Block, BlockNumber, Hash, Header, Nonce};
use sc_client_api::{
    AuxStore, Backend as BackendT, BlockBackend, BlockchainEvents, KeysIter, PairsIter,
    UsageProvider,
};
use sp_api::{CallApiAt, ProvideRuntimeApi};
use sp_blockchain::{HeaderBackend, HeaderMetadata};
use sp_consensus::BlockStatus;
use sp_core::H256;
use sp_runtime::{
    Justifications, OpaqueExtrinsic,
    generic::SignedBlock,
    traits::{BlakeTwo256, Block as BlockT, NumberFor},
};
use sp_state_machine::Backend as StateBackend;
use sp_storage::{ChildInfo, StorageData, StorageKey};
use sp_trie::MerkleValue;
use std::sync::Arc;

pub type FullBackend = sc_service::TFullBackend<Block>;

/// A specialized `WasmExecutor` intended to use across substrate node. It provides all required
/// HostFunctions.
pub type RuntimeExecutor =
    sc_executor::WasmExecutor<(sp_io::SubstrateHostFunctions, ExtendHostFunctions)>;

pub type FullClient<RuntimeApi> = sc_service::TFullClient<Block, RuntimeApi, RuntimeExecutor>;

/// Only enable the benchmarking host functions when we actually want to benchmark.
#[cfg(feature = "runtime-benchmarks")]
pub type ExtendHostFunctions = (
    frame_benchmarking::benchmarking::HostFunctions,
    gear_ri::gear_ri::HostFunctions,
    gear_ri::sandbox::HostFunctions,
    sp_crypto_ec_utils::bls12_381::host_calls::HostFunctions,
    gear_ri::gear_bls_12_381::HostFunctions,
);
/// Otherwise we only use the default Substrate host functions.
#[cfg(not(feature = "runtime-benchmarks"))]
pub type ExtendHostFunctions = (
    gear_ri::gear_ri::HostFunctions,
    gear_ri::sandbox::HostFunctions,
    sp_crypto_ec_utils::bls12_381::host_calls::HostFunctions,
    gear_ri::gear_bls_12_381::HostFunctions,
);

/// A set of APIs that polkadot-like runtimes must implement.
///
/// This trait has no methods or associated type. It is a concise marker for all the trait bounds
/// that it contains.
pub trait RuntimeApiCollection:
    sp_transaction_pool::runtime_api::TaggedTransactionQueue<Block>
    + sp_api::ApiExt<Block>
    + sp_consensus_babe::BabeApi<Block>
    + sp_consensus_grandpa::GrandpaApi<Block>
    + sp_block_builder::BlockBuilder<Block>
    + substrate_frame_rpc_system::AccountNonceApi<Block, AccountId, Nonce>
    + pallet_transaction_payment_rpc_runtime_api::TransactionPaymentApi<Block, Balance>
    + sp_api::Metadata<Block>
    + sp_offchain::OffchainWorkerApi<Block>
    + sp_session::SessionKeys<Block>
    + sp_authority_discovery::AuthorityDiscoveryApi<Block>
    + pallet_gear_rpc_runtime_api::GearApi<Block>
    + pallet_gear_staking_rewards_rpc_runtime_api::GearStakingRewardsApi<Block>
    + pallet_gear_builtin_rpc_runtime_api::GearBuiltinApi<Block>
    + pallet_gear_eth_bridge_rpc_runtime_api::GearEthBridgeApi<Block>
{
}

impl<Api> RuntimeApiCollection for Api where
    Api: sp_transaction_pool::runtime_api::TaggedTransactionQueue<Block>
        + sp_api::ApiExt<Block>
        + sp_consensus_babe::BabeApi<Block>
        + sp_consensus_grandpa::GrandpaApi<Block>
        + sp_block_builder::BlockBuilder<Block>
        + substrate_frame_rpc_system::AccountNonceApi<Block, AccountId, Nonce>
        + pallet_transaction_payment_rpc_runtime_api::TransactionPaymentApi<Block, Balance>
        + sp_api::Metadata<Block>
        + sp_offchain::OffchainWorkerApi<Block>
        + sp_session::SessionKeys<Block>
        + sp_authority_discovery::AuthorityDiscoveryApi<Block>
        + pallet_gear_rpc_runtime_api::GearApi<Block>
        + pallet_gear_staking_rewards_rpc_runtime_api::GearStakingRewardsApi<Block>
        + pallet_gear_builtin_rpc_runtime_api::GearBuiltinApi<Block>
        + pallet_gear_eth_bridge_rpc_runtime_api::GearEthBridgeApi<Block>
{
}

/// Trait that abstracts over all available client implementations.
///
/// For a concrete type there exists [`Client`].
pub trait AbstractClient<Block, Backend>:
    BlockchainEvents<Block>
    + Sized
    + Send
    + Sync
    + ProvideRuntimeApi<Block>
    + HeaderBackend<Block>
    + CallApiAt<Block>
where
    Block: BlockT,
    Backend: BackendT<Block>,
    Backend::State: StateBackend<BlakeTwo256>,
    Self::Api: RuntimeApiCollection,
{
}

impl<Block, Backend, Client> AbstractClient<Block, Backend> for Client
where
    Block: BlockT,
    Backend: BackendT<Block>,
    Backend::State: StateBackend<BlakeTwo256>,
    Client: BlockchainEvents<Block>
        + ProvideRuntimeApi<Block>
        + HeaderBackend<Block>
        + Sized
        + Send
        + Sync
        + CallApiAt<Block>,
    Client::Api: RuntimeApiCollection,
{
}

/// Execute something with the client instance.
///
/// As there are multiple chains in Gear, there can exist different kinds of client types.
/// As these client types differ in the generics that are being used, we can not easily return
/// them from a function. For returning them from a function there exists [`Client`].
/// However, the problem of how to use this client instance still exists.
/// This trait "solves" it in a dirty way. It requires a type to implement this trait and than
/// the [`execute_with_client`](ExecuteWithClient::execute_with_client) function can be called
/// with any possible client instance.
///
/// In a perfect world, we could make a closure work in this way.
pub trait ExecuteWithClient {
    /// The return type when calling this instance.
    type Output;

    /// Execute whatever should be executed with the given client instance.
    fn execute_with_client<Client, Api, Backend>(self, client: Arc<Client>) -> Self::Output
    where
        Backend: BackendT<Block> + 'static,
        Backend::State: StateBackend<BlakeTwo256>,
        Api: crate::RuntimeApiCollection,
        Client: AbstractClient<Block, Backend, Api = Api>
            + 'static
            + HeaderMetadata<
                sp_runtime::generic::Block<
                    sp_runtime::generic::Header<u32, BlakeTwo256>,
                    OpaqueExtrinsic,
                >,
                Error = sp_blockchain::Error,
            >
            + AuxStore
            + UsageProvider<
                sp_runtime::generic::Block<
                    sp_runtime::generic::Header<u32, BlakeTwo256>,
                    OpaqueExtrinsic,
                >,
            >;
}

/// A handle to a Gear client instance.
///
/// The Gear service supports multiple different runtimes (basic, standalone, etc).
/// As each runtime has a specialized client, we need to hide them behind a trait.
///
/// When wanting to work with the inner client, you need to use `execute_with`.
pub trait ClientHandle {
    /// Execute the given something with the client.
    fn execute_with<T: ExecuteWithClient>(&self, t: T) -> T::Output;
}

macro_rules! with_client {
    {
        $self:ident,
        $client:ident,
        {
            $( $code:tt )*
        }
    } => {
        match $self {
            #[cfg(feature = "vara-native")]
            Self::Vara($client) => { $( $code )* },
        }
    }
}

/// A client instance of Gear.
#[derive(Clone)]
pub enum Client {
    #[cfg(feature = "vara-native")]
    Vara(Arc<crate::FullClient<vara_runtime::RuntimeApi>>),
}

#[cfg(feature = "vara-native")]
impl From<Arc<crate::FullClient<vara_runtime::RuntimeApi>>> for Client {
    fn from(client: Arc<crate::FullClient<vara_runtime::RuntimeApi>>) -> Self {
        Self::Vara(client)
    }
}

impl ClientHandle for Client {
    fn execute_with<T: ExecuteWithClient>(&self, t: T) -> T::Output {
        with_client! {
            self,
            client,
            {
                T::execute_with_client::<_, _, FullBackend>(t, client.clone())
            }
        }
    }
}

impl UsageProvider<Block> for Client {
    fn usage_info(&self) -> sc_client_api::ClientInfo<Block> {
        with_client! {
            self,
            client,
            {
                client.usage_info()
            }
        }
    }
}

impl BlockBackend<Block> for Client {
    fn block_body(
        &self,
        id: <Block as BlockT>::Hash,
    ) -> sp_blockchain::Result<Option<Vec<<Block as BlockT>::Extrinsic>>> {
        with_client! {
            self,
            client,
            {
                client.block_body(id)
            }
        }
    }

    fn block(&self, id: H256) -> sp_blockchain::Result<Option<SignedBlock<Block>>> {
        with_client! {
            self,
            client,
            {
                client.block(id)
            }
        }
    }

    fn block_status(&self, id: H256) -> sp_blockchain::Result<BlockStatus> {
        with_client! {
            self,
            client,
            {
                client.block_status(id)
            }
        }
    }

    fn justifications(
        &self,
        id: <Block as BlockT>::Hash,
    ) -> sp_blockchain::Result<Option<Justifications>> {
        with_client! {
            self,
            client,
            {
                client.justifications(id)
            }
        }
    }

    fn block_hash(
        &self,
        number: NumberFor<Block>,
    ) -> sp_blockchain::Result<Option<<Block as BlockT>::Hash>> {
        with_client! {
            self,
            client,
            {
                client.block_hash(number)
            }
        }
    }

    fn indexed_transaction(
        &self,
        id: <Block as BlockT>::Hash,
    ) -> sp_blockchain::Result<Option<Vec<u8>>> {
        with_client! {
            self,
            client,
            {
                client.indexed_transaction(id)
            }
        }
    }

    fn block_indexed_body(
        &self,
        id: <Block as BlockT>::Hash,
    ) -> sp_blockchain::Result<Option<Vec<Vec<u8>>>> {
        with_client! {
            self,
            client,
            {
                client.block_indexed_body(id)
            }
        }
    }

    fn requires_full_sync(&self) -> bool {
        with_client! {
            self,
            client,
            {
                client.requires_full_sync()
            }
        }
    }
}

impl sc_client_api::StorageProvider<Block, crate::FullBackend> for Client {
    fn storage(
        &self,
        id: <Block as BlockT>::Hash,
        key: &StorageKey,
    ) -> sp_blockchain::Result<Option<StorageData>> {
        with_client! {
            self,
            client,
            {
                client.storage(id, key)
            }
        }
    }

    fn storage_keys(
        &self,
        hash: <Block as BlockT>::Hash,
        prefix: Option<&StorageKey>,
        start_key: Option<&StorageKey>,
    ) -> sp_blockchain::Result<
        KeysIter<<crate::FullBackend as sc_client_api::Backend<Block>>::State, Block>,
    > {
        with_client! {
            self,
            client,
            {
                client.storage_keys(hash, prefix, start_key)
            }
        }
    }

    fn storage_hash(
        &self,
        id: <Block as BlockT>::Hash,
        key: &StorageKey,
    ) -> sp_blockchain::Result<Option<<Block as BlockT>::Hash>> {
        with_client! {
            self,
            client,
            {
                client.storage_hash(id, key)
            }
        }
    }

    fn storage_pairs(
        &self,
        id: <Block as BlockT>::Hash,
        prefix: Option<&StorageKey>,
        start_key: Option<&StorageKey>,
    ) -> sp_blockchain::Result<
        PairsIter<<crate::FullBackend as sc_client_api::Backend<Block>>::State, Block>,
    > {
        with_client! {
            self,
            client,
            {
                client.storage_pairs(id, prefix, start_key)
            }
        }
    }

    fn child_storage(
        &self,
        id: <Block as BlockT>::Hash,
        child_info: &ChildInfo,
        key: &StorageKey,
    ) -> sp_blockchain::Result<Option<StorageData>> {
        with_client! {
            self,
            client,
            {
                client.child_storage(id, child_info, key)
            }
        }
    }

    fn child_storage_keys(
        &self,
        id: <Block as BlockT>::Hash,
        child_info: ChildInfo,
        prefix: Option<&StorageKey>,
        start_key: Option<&StorageKey>,
    ) -> sp_blockchain::Result<
        KeysIter<<crate::FullBackend as sc_client_api::Backend<Block>>::State, Block>,
    > {
        with_client! {
            self,
            client,
            {
                client.child_storage_keys(id, child_info, prefix, start_key)
            }
        }
    }

    fn child_storage_hash(
        &self,
        id: <Block as BlockT>::Hash,
        child_info: &ChildInfo,
        key: &StorageKey,
    ) -> sp_blockchain::Result<Option<<Block as BlockT>::Hash>> {
        with_client! {
            self,
            client,
            {
                client.child_storage_hash(id, child_info, key)
            }
        }
    }

    fn closest_merkle_value(
        &self,
        hash: <Block as BlockT>::Hash,
        key: &StorageKey,
    ) -> sp_blockchain::Result<Option<MerkleValue<<Block as BlockT>::Hash>>> {
        with_client! {
            self,
            client,
            {
                client.state_at(hash)?
                    .closest_merkle_value(&key.0)
                    .map_err(|e| sp_blockchain::Error::from_state(Box::new(e)))
            }
        }
    }

    fn child_closest_merkle_value(
        &self,
        hash: <Block as BlockT>::Hash,
        child_info: &ChildInfo,
        key: &StorageKey,
    ) -> sp_blockchain::Result<Option<MerkleValue<<Block as BlockT>::Hash>>> {
        with_client! {
            self,
            client,
            {
                client.state_at(hash)?
                .child_closest_merkle_value(child_info, &key.0)
                    .map_err(|e| sp_blockchain::Error::from_state(Box::new(e)))
            }
        }
    }
}

impl sp_blockchain::HeaderBackend<Block> for Client {
    fn header(&self, id: H256) -> sp_blockchain::Result<Option<Header>> {
        with_client! {
            self,
            client,
            {
                client.header(id)
            }
        }
    }

    fn info(&self) -> sp_blockchain::Info<Block> {
        with_client! {
            self,
            client,
            {
                client.info()
            }
        }
    }

    fn status(&self, id: H256) -> sp_blockchain::Result<sp_blockchain::BlockStatus> {
        with_client! {
            self,
            client,
            {
                client.status(id)
            }
        }
    }

    fn number(&self, hash: Hash) -> sp_blockchain::Result<Option<BlockNumber>> {
        with_client! {
            self,
            client,
            {
                client.number(hash)
            }
        }
    }

    fn hash(&self, number: BlockNumber) -> sp_blockchain::Result<Option<Hash>> {
        with_client! {
            self,
            client,
            {
                client.hash(number)
            }
        }
    }
}
