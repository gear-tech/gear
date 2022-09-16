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

use sc_client_api::{Backend as BackendT, BlockBackend, UsageProvider, ExecutorProvider};
use sc_executor::{NativeElseWasmExecutor, NativeExecutionDispatch};
use sc_finality_grandpa::SharedVoterState;
use sc_keystore::LocalKeystore;
use sc_service::{
    error::Error as ServiceError, ChainSpec, Configuration, PartialComponents, TaskManager,
};
use sc_telemetry::{Telemetry, TelemetryWorker};
use sp_api::ConstructRuntimeApi;
use sp_runtime::{traits::BlakeTwo256, OpaqueExtrinsic};
use sp_trie::PrefixedMemoryDB;
use std::{sync::Arc, time::Duration};

pub use client::*;

pub use sc_client_api::AuxStore;
pub use sp_blockchain::{HeaderBackend, HeaderMetadata};
pub use sp_consensus_babe::BabeApi;

#[cfg(feature = "gear-native")]
pub use gear_runtime;
#[cfg(feature = "vara-native")]
pub use vara_runtime;

pub mod chain_spec;
mod client;

pub mod rpc;

pub trait IdentifyVariant {
    /// Returns `true` if this is a configuration for gear network.
    fn is_gear(&self) -> bool;

    /// Returns `true` if this is a configuration for the vara network.
    fn is_vara(&self) -> bool;

    /// Returns true if this configuration is for a development network.
    fn is_dev(&self) -> bool;
}

impl IdentifyVariant for Box<dyn ChainSpec> {
    fn is_gear(&self) -> bool {
        self.id().to_lowercase().starts_with("gear")
    }
    fn is_vara(&self) -> bool {
        self.id().to_lowercase().starts_with("vara")
    }
    fn is_dev(&self) -> bool {
        self.id().ends_with("dev")
    }
}

type FullSelectChain = sc_consensus::LongestChain<FullBackend, Block>;
type FullGrandpaBlockImport<RuntimeApi, ExecutorDispatch, ChainSelection = FullSelectChain> =
    sc_finality_grandpa::GrandpaBlockImport<
        FullBackend,
        Block,
        FullClient<RuntimeApi, ExecutorDispatch>,
        ChainSelection,
    >;
type FullBabeBlockImport<RuntimeApi, ExecutorDispatch, ChainSelection> = (
    sc_consensus_babe::BabeBlockImport<
        Block,
        FullClient<RuntimeApi, ExecutorDispatch>,
        FullGrandpaBlockImport<RuntimeApi, ExecutorDispatch, ChainSelection>,
    >,
    sc_consensus_babe::BabeLink<Block>,
);

macro_rules! chain_ops {
    ($config:expr, $scope:ident, $executor:ident, $variant:ident) => {{
        let PartialComponents {
            client,
            backend,
            import_queue,
            task_manager,
            ..
        } = new_partial::<$scope::RuntimeApi, $executor>($config)?;

        Ok((
            Arc::new(Client::$variant(client)),
            backend,
            import_queue,
            task_manager,
        ))
    }};
}

/// Builds a new object suitable for chain operations.
#[allow(clippy::type_complexity)]
pub fn new_chain_ops(
    config: &Configuration,
) -> Result<
    (
        Arc<Client>,
        Arc<FullBackend>,
        sc_consensus::BasicQueue<Block, PrefixedMemoryDB<BlakeTwo256>>,
        TaskManager,
    ),
    ServiceError,
> {
    match &config.chain_spec {
        #[cfg(feature = "gear-native")]
        spec if spec.is_gear() => {
            chain_ops!(config, gear_runtime, GearExecutorDispatch, Gear)
        }
        #[cfg(feature = "vara-native")]
        spec if spec.is_vara() => {
            chain_ops!(config, vara_runtime, VaraExecutorDispatch, Vara)
        }
        _ => Err("invalid chain spec".into()),
    }
}

type OtherPartial<RuntimeApi, ExecutorDispatch, ChainSelection = FullSelectChain> = (
    FullGrandpaBlockImport<RuntimeApi, ExecutorDispatch, ChainSelection>,
    sc_finality_grandpa::LinkHalf<Block, FullClient<RuntimeApi, ExecutorDispatch>, ChainSelection>,
    Option<Telemetry>,
    FullBabeBlockImport<RuntimeApi, ExecutorDispatch, ChainSelection>,
);

/// Creates PartialComponents for a node.
/// Enables chain operations for cases when full node is unnecessary.
#[allow(clippy::type_complexity)]
pub fn new_partial<RuntimeApi, ExecutorDispatch>(
    config: &Configuration,
) -> Result<
    PartialComponents<
        FullClient<RuntimeApi, ExecutorDispatch>,
        FullBackend,
        FullSelectChain,
        sc_consensus::DefaultImportQueue<Block, FullClient<RuntimeApi, ExecutorDispatch>>,
        sc_transaction_pool::FullPool<Block, FullClient<RuntimeApi, ExecutorDispatch>>,
        OtherPartial<RuntimeApi, ExecutorDispatch, FullSelectChain>,
    >,
    ServiceError,
>
where
    RuntimeApi: ConstructRuntimeApi<Block, FullClient<RuntimeApi, ExecutorDispatch>>
        + Send
        + Sync
        + 'static,
    RuntimeApi::RuntimeApi:
        RuntimeApiCollection<StateBackend = sc_client_api::StateBackendFor<FullBackend, Block>>,
    ExecutorDispatch: NativeExecutionDispatch + 'static,
{
    if config.keystore_remote.is_some() {
        return Err(ServiceError::Other(
            "Remote Keystores are not supported.".into(),
        ));
    }

    let telemetry = config
        .telemetry_endpoints
        .clone()
        .filter(|x| !x.is_empty())
        .map(|endpoints| -> Result<_, sc_telemetry::Error> {
            let worker = TelemetryWorker::new(16)?;
            let telemetry = worker.handle().new_telemetry(endpoints);
            Ok((worker, telemetry))
        })
        .transpose()?;

    let executor = NativeElseWasmExecutor::<ExecutorDispatch>::new(
        config.wasm_method,
        config.default_heap_pages,
        config.max_runtime_instances,
        config.runtime_cache_size,
    );

    let (client, backend, keystore_container, task_manager) =
        sc_service::new_full_parts::<Block, RuntimeApi, _>(
            config,
            telemetry.as_ref().map(|(_, telemetry)| telemetry.handle()),
            executor,
        )?;
    let client = Arc::new(client);

    let telemetry = telemetry.map(|(worker, telemetry)| {
        task_manager
            .spawn_handle()
            .spawn("telemetry", None, worker.run());
        telemetry
    });

    let select_chain = sc_consensus::LongestChain::new(backend.clone());

    let transaction_pool = sc_transaction_pool::BasicPool::new_full(
        config.transaction_pool.clone(),
        config.role.is_authority().into(),
        config.prometheus_registry(),
        task_manager.spawn_essential_handle(),
        client.clone(),
    );

    let (grandpa_block_import, grandpa_link) = sc_finality_grandpa::block_import(
        client.clone(),
        &(client.clone() as Arc<_>),
        select_chain.clone(),
        telemetry.as_ref().map(|x| x.handle()),
    )?;

    let (import_queue, babe_block_import_setup) = {
        let babe_config = sc_consensus_babe::configuration(&*client)?;
        let (babe_block_import, babe_link) = sc_consensus_babe::block_import(
            babe_config,
            grandpa_block_import.clone(),
            client.clone(),
        )?;
        let slot_duration = babe_link.config().slot_duration();
        (
            sc_consensus_babe::import_queue(
                babe_link.clone(),
                babe_block_import.clone(),
                Some(Box::new(grandpa_block_import.clone())),
                client.clone(),
                select_chain.clone(),
                move |_, ()| async move {
                    let timestamp = sp_timestamp::InherentDataProvider::from_system_time();

                    let slot =
                    sp_consensus_babe::inherents::InherentDataProvider::from_timestamp_and_slot_duration(
                        *timestamp,
                        slot_duration,
                    );

                    Ok((timestamp, slot))
                },
                &task_manager.spawn_essential_handle(),
                config.prometheus_registry(),
                sp_consensus::CanAuthorWithNativeVersion::new(client.executor().clone()),
                telemetry.as_ref().map(|x| x.handle()),
            )?,
            (babe_block_import, babe_link),
        )
    };

    let partial = PartialComponents {
        client,
        backend,
        task_manager,
        import_queue,
        keystore_container,
        select_chain,
        transaction_pool,
        other: (
            grandpa_block_import,
            grandpa_link,
            telemetry,
            babe_block_import_setup,
        ),
    };

    Ok(partial)
}

fn remote_keystore(_url: &str) -> Result<Arc<LocalKeystore>, &'static str> {
    // FIXME: here would the concrete keystore be built,
    //        must return a concrete type (NOT `LocalKeystore`) that
    //        implements `CryptoStore` and `SyncCryptoStore`
    Err("Remote Keystore not supported.")
}

/// Build a full node for different chain "flavors".
///
/// The actual "flavor", aka if it will use `Gear`, `Vara` etc. is determined based on
/// [`IdentifyVariant`] using the chain spec.
pub fn build_full(config: Configuration) -> Result<TaskManager, ServiceError> {
    match &config.chain_spec {
        #[cfg(feature = "gear-native")]
        spec if spec.is_gear() => {
            new_full::<gear_runtime::RuntimeApi, GearExecutorDispatch>(config)
        }
        #[cfg(feature = "vara-native")]
        spec if spec.is_vara() => {
            new_full::<vara_runtime::RuntimeApi, VaraExecutorDispatch>(config)
        }
        _ => Err(ServiceError::Other("Invalid chain spec".into())),
    }
}

/// Builds a new service for a full client.
pub fn new_full<RuntimeApi, ExecutorDispatch>(
    mut config: Configuration,
) -> Result<TaskManager, ServiceError>
where
    RuntimeApi: ConstructRuntimeApi<Block, FullClient<RuntimeApi, ExecutorDispatch>>
        + Send
        + Sync
        + 'static,
    RuntimeApi::RuntimeApi:
        RuntimeApiCollection<StateBackend = sc_client_api::StateBackendFor<FullBackend, Block>>,
    ExecutorDispatch: NativeExecutionDispatch + 'static,
{
    let PartialComponents {
        client,
        backend,
        mut task_manager,
        import_queue,
        mut keystore_container,
        select_chain,
        transaction_pool,
        other,
    } = new_partial::<RuntimeApi, ExecutorDispatch>(&config)?;

    let (_, grandpa_link, mut telemetry, (babe_block_import, babe_link)) = other;

    if let Some(url) = &config.keystore_remote {
        match remote_keystore(url) {
            Ok(k) => keystore_container.set_remote_keystore(k),
            Err(e) => {
                return Err(ServiceError::Other(format!(
                    "Error hooking up remote keystore for {}: {}",
                    url, e
                )))
            }
        };
    }
    let grandpa_protocol_name = sc_finality_grandpa::protocol_standard_name(
        &client
            .block_hash(0)
            .ok()
            .flatten()
            .expect("Genesis block exists; qed"),
        &config.chain_spec,
    );

    config
        .network
        .extra_sets
        .push(sc_finality_grandpa::grandpa_peers_set_config(
            grandpa_protocol_name.clone(),
        ));
    let warp_sync = Arc::new(sc_finality_grandpa::warp_proof::NetworkProvider::new(
        backend.clone(),
        grandpa_link.shared_authority_set().clone(),
        Vec::default(),
    ));

    let (network, system_rpc_tx, network_starter) =
        sc_service::build_network(sc_service::BuildNetworkParams {
            config: &config,
            client: client.clone(),
            transaction_pool: transaction_pool.clone(),
            spawn_handle: task_manager.spawn_handle(),
            import_queue,
            block_announce_validator_builder: None,
            warp_sync: Some(warp_sync),
        })?;

    if config.offchain_worker.enabled {
        sc_service::build_offchain_workers(
            &config,
            task_manager.spawn_handle(),
            client.clone(),
            network.clone(),
        );
    }

    let role = config.role.clone();
    let force_authoring = config.force_authoring;
    let backoff_authoring_blocks: Option<()> = None;
    let name = config.network.node_name.clone();
    let enable_grandpa = !config.disable_grandpa;
    let prometheus_registry = config.prometheus_registry().cloned();

    let rpc_extensions_builder = {
        let client = client.clone();
        let pool = transaction_pool.clone();

        Box::new(move |deny_unsafe, _| {
            let deps = crate::rpc::FullDeps {
                client: client.clone(),
                pool: pool.clone(),
                deny_unsafe,
            };
            crate::rpc::create_full(deps).map_err(Into::into)
        })
    };

    let _rpc_handlers = sc_service::spawn_tasks(sc_service::SpawnTasksParams {
        network: network.clone(),
        client: client.clone(),
        keystore: keystore_container.sync_keystore(),
        task_manager: &mut task_manager,
        transaction_pool: transaction_pool.clone(),
        rpc_builder: rpc_extensions_builder,
        backend,
        system_rpc_tx,
        config,
        telemetry: telemetry.as_mut(),
    })?;

    if role.is_authority() {
        let proposer_factory = sc_basic_authorship::ProposerFactory::new(
            task_manager.spawn_handle(),
            client.clone(),
            transaction_pool,
            prometheus_registry.as_ref(),
            telemetry.as_ref().map(|x| x.handle()),
        );

        let can_author_with = sp_consensus::CanAuthorWithNativeVersion::new(client.executor().clone());

        {
            let slot_duration = babe_link.config().slot_duration();

            let babe_config = sc_consensus_babe::BabeParams {
                keystore: keystore_container.sync_keystore(),
                client,
                select_chain,
                env: proposer_factory,
                block_import: babe_block_import,
                sync_oracle: network.clone(),
                justification_sync_link: network.clone(),
                create_inherent_data_providers: move |_, ()| async move {
                    let timestamp = sp_timestamp::InherentDataProvider::from_system_time();

                    let slot =
                            sp_consensus_babe::inherents::InherentDataProvider::from_timestamp_and_slot_duration(
                                *timestamp,
                                slot_duration,
                            );

                    Ok((timestamp, slot))
                },
                force_authoring,
                backoff_authoring_blocks,
                babe_link,
                can_author_with,
                block_proposal_slot_portion: sc_consensus_babe::SlotProportion::new(2f32 / 3f32), // Substrate suggests 0.5
                max_block_proposal_slot_portion: None,
                telemetry: telemetry.as_ref().map(|x| x.handle()),
            };

            let babe = sc_consensus_babe::start_babe(babe_config)?;
            task_manager.spawn_essential_handle().spawn_blocking(
                "babe",
                Some("block-authoring"),
                babe,
            );
        }
    }

    // if the node isn't actively participating in consensus then it doesn't
    // need a keystore, regardless of which protocol we use below.
    let keystore = if role.is_authority() {
        Some(keystore_container.sync_keystore())
    } else {
        None
    };

    let grandpa_config = sc_finality_grandpa::Config {
        // FIXME #1578 make this available through chainspec
        gossip_duration: Duration::from_millis(333),
        justification_period: 512,
        name: Some(name),
        observer_enabled: false,
        keystore,
        local_role: role,
        telemetry: telemetry.as_ref().map(|x| x.handle()),
        protocol_name: grandpa_protocol_name,
    };

    if enable_grandpa {
        // start the full GRANDPA voter
        // NOTE: non-authorities could run the GRANDPA observer protocol, but at
        // this point the full voter should provide better guarantees of block
        // and vote data availability than the observer. The observer has not
        // been tested extensively yet and having most nodes in a network run it
        // could lead to finality stalls.
        let grandpa_config = sc_finality_grandpa::GrandpaParams {
            config: grandpa_config,
            link: grandpa_link,
            network,
            voting_rule: sc_finality_grandpa::VotingRulesBuilder::default().build(),
            prometheus_registry,
            shared_voter_state: SharedVoterState::empty(),
            telemetry: telemetry.as_ref().map(|x| x.handle()),
        };

        // the GRANDPA voter task is considered infallible, i.e.
        // if it fails we take down the service with it.
        task_manager.spawn_essential_handle().spawn_blocking(
            "grandpa-voter",
            None,
            sc_finality_grandpa::run_grandpa_voter(grandpa_config)?,
        );
    }

    network_starter.start_network();
    Ok(task_manager)
}

struct RevertConsensus {
    blocks: BlockNumber,
    backend: Arc<FullBackend>,
}

impl ExecuteWithClient for RevertConsensus {
    type Output = sp_blockchain::Result<()>;

    fn execute_with_client<Client, Api, Backend>(self, client: Arc<Client>) -> Self::Output
    where
        <Api as sp_api::ApiExt<Block>>::StateBackend: sp_api::StateBackend<BlakeTwo256>,
        Backend: BackendT<Block> + 'static,
        Backend::State: sp_api::StateBackend<BlakeTwo256>,
        Api: RuntimeApiCollection<StateBackend = Backend::State>,
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
            >,
    {
        sc_consensus_babe::revert(client.clone(), self.backend, self.blocks)?;
        sc_finality_grandpa::revert(client, self.blocks)?;
        Ok(())
    }
}

/// Reverts the node state down to at most the last finalized block.
///
/// In particular this reverts:
/// - Low level Babe and Grandpa consensus data.
pub fn revert_backend(
    client: Arc<Client>,
    backend: Arc<FullBackend>,
    blocks: BlockNumber,
    _config: Configuration,
) -> Result<(), ServiceError> {
    client.execute_with(RevertConsensus { blocks, backend })?;

    Ok(())
}
