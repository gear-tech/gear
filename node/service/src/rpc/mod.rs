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

//! RPC module for node.

#![warn(missing_docs)]

use std::sync::Arc;

use jsonrpsee::RpcModule;
use runtime_primitives::{AccountId, Balance, Block, BlockNumber, Hash, Nonce};
use sc_client_api::{AuxStore, Backend, BlockBackend, StorageProvider, backend::StateBackend};
use sc_consensus_babe::BabeWorkerHandle;
use sc_consensus_grandpa::{
    FinalityProofProvider, GrandpaJustificationStream, SharedAuthoritySet, SharedVoterState,
};
use sc_rpc::SubscriptionTaskExecutor;
use sc_transaction_pool_api::TransactionPool;
use sp_api::ProvideRuntimeApi;
use sp_block_builder::BlockBuilder;
use sp_blockchain::{Error as BlockChainError, HeaderBackend, HeaderMetadata};
use sp_consensus::SelectChain;
use sp_consensus_babe::BabeApi;
use sp_keystore::KeystorePtr;

mod runtime_info;

/// Extra dependencies for BABE.
pub struct BabeDeps {
    /// A handle to the BABE worker for issuing requests.
    pub babe_worker_handle: BabeWorkerHandle<Block>,
    /// The keystore that manages the keys of the node.
    pub keystore: KeystorePtr,
}

/// Extra dependencies for GRANDPA
pub struct GrandpaDeps<B> {
    /// Voting round info.
    pub shared_voter_state: SharedVoterState,
    /// Authority set info.
    pub shared_authority_set: SharedAuthoritySet<Hash, BlockNumber>,
    /// Receives notifications about justification events from Grandpa.
    pub justification_stream: GrandpaJustificationStream<Block>,
    /// Executor to drive the subscription manager in the Grandpa RPC handler.
    pub subscription_executor: SubscriptionTaskExecutor,
    /// Finality proof provider.
    pub finality_provider: Arc<FinalityProofProvider<B, Block>>,
}

/// Extra dependencies for GEAR.
pub struct GearDeps {
    /// gas allowance block limit multiplier.
    pub allowance_multiplier: u64,
    /// maximum batch size for rpc call.
    pub max_batch_size: u64,
}

/// Full client dependencies.
pub struct FullDeps<C, P, SC, B> {
    /// The client instance to use.
    pub client: Arc<C>,
    /// Transaction pool instance.
    pub pool: Arc<P>,
    /// The SelectChain Strategy
    pub select_chain: SC,
    /// A copy of the chain spec.
    pub chain_spec: Box<dyn sc_chain_spec::ChainSpec>,
    /// BABE specific dependencies.
    pub babe: BabeDeps,
    /// GRANDPA specific dependencies.
    pub grandpa: GrandpaDeps<B>,
    /// GEAR specific dependencies.
    pub gear: GearDeps,
    /// The backend used by the node.
    pub backend: Arc<B>,
}

/// Instantiate all Full RPC extensions.
pub fn create_full<C, P, SC, B>(
    FullDeps {
        client,
        pool,
        select_chain,
        chain_spec,
        babe,
        grandpa,
        gear,
        backend,
    }: FullDeps<C, P, SC, B>,
) -> Result<RpcModule<()>, Box<dyn std::error::Error + Send + Sync>>
where
    C: ProvideRuntimeApi<Block>
        + BlockBackend<Block>
        + HeaderBackend<Block>
        + AuxStore
        + HeaderMetadata<Block, Error = BlockChainError>
        + StorageProvider<Block, B>
        + Sync
        + Send
        + 'static,
    C::Api: substrate_frame_rpc_system::AccountNonceApi<Block, AccountId, Nonce>,
    C::Api: pallet_gear_rpc::GearRuntimeApi<Block>,
    C::Api: pallet_gear_builtin_rpc::GearBuiltinRuntimeApi<Block>,
    C::Api: pallet_gear_eth_bridge_rpc::GearEthBridgeRuntimeApi<Block>,
    C::Api: pallet_gear_staking_rewards_rpc::GearStakingRewardsRuntimeApi<Block>,
    C::Api: pallet_transaction_payment_rpc::TransactionPaymentRuntimeApi<Block, Balance>,
    C::Api: BabeApi<Block>,
    C::Api: BlockBuilder<Block>,
    P: TransactionPool + 'static,
    SC: SelectChain<Block> + 'static,
    B: Backend<Block> + Send + Sync + 'static,
    B::State: StateBackend<sp_runtime::traits::HashingFor<Block>>,
{
    use pallet_gear_builtin_rpc::{GearBuiltin, GearBuiltinApiServer};
    use pallet_gear_eth_bridge_rpc::{GearEthBridge, GearEthBridgeApiServer};
    use pallet_gear_rpc::{Gear, GearApiServer};
    use pallet_gear_staking_rewards_rpc::{GearStakingRewards, GearStakingRewardsApiServer};
    use pallet_transaction_payment_rpc::{TransactionPayment, TransactionPaymentApiServer};
    use runtime_info::{RuntimeInfoApi, RuntimeInfoServer};
    use sc_consensus_babe_rpc::{Babe, BabeApiServer};
    use sc_consensus_grandpa_rpc::{Grandpa, GrandpaApiServer};
    use sc_rpc::dev::{Dev, DevApiServer};
    use sc_sync_state_rpc::{SyncState, SyncStateApiServer};
    use substrate_frame_rpc_system::{System, SystemApiServer};
    use substrate_state_trie_migration_rpc::{StateMigration, StateMigrationApiServer};

    let mut io = RpcModule::new(());

    let BabeDeps {
        keystore,
        babe_worker_handle,
    } = babe;
    let GrandpaDeps {
        shared_voter_state,
        shared_authority_set,
        justification_stream,
        subscription_executor,
        finality_provider,
    } = grandpa;

    let GearDeps {
        allowance_multiplier,
        max_batch_size,
    } = gear;

    io.merge(System::new(client.clone(), pool).into_rpc())?;
    io.merge(TransactionPayment::new(client.clone()).into_rpc())?;
    io.merge(
        Babe::new(
            client.clone(),
            babe_worker_handle.clone(),
            keystore,
            select_chain,
        )
        .into_rpc(),
    )?;
    io.merge(
        Grandpa::new(
            subscription_executor,
            shared_authority_set.clone(),
            shared_voter_state,
            justification_stream,
            finality_provider,
        )
        .into_rpc(),
    )?;

    io.merge(
        SyncState::new(
            chain_spec,
            client.clone(),
            shared_authority_set,
            babe_worker_handle,
        )?
        .into_rpc(),
    )?;

    io.merge(StateMigration::new(client.clone(), backend).into_rpc())?;
    io.merge(Dev::new(client.clone()).into_rpc())?;

    io.merge(Gear::new(client.clone(), allowance_multiplier, max_batch_size).into_rpc())?;

    io.merge(RuntimeInfoApi::<C, Block, B>::new(client.clone()).into_rpc())?;

    io.merge(GearStakingRewards::new(client.clone()).into_rpc())?;

    io.merge(GearBuiltin::new(client.clone()).into_rpc())?;

    io.merge(GearEthBridge::new(client).into_rpc())?;

    Ok(io)
}
