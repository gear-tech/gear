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

#![allow(clippy::redundant_clone)]

use frame_benchmarking_cli::SUBSTRATE_REFERENCE_HARDWARE;
use sc_client_api::{Backend as BackendT, BlockBackend, UsageProvider};
use sc_executor::{NativeElseWasmExecutor, NativeExecutionDispatch};
use sc_network::NetworkService;
use sc_network_common::sync::warp::WarpSyncParams;
use sc_network_sync::SyncingService;
use sc_service::{
    error::Error as ServiceError, ChainSpec, Configuration, PartialComponents, RpcHandlers,
    TaskManager,
};
use sc_telemetry::{Telemetry, TelemetryWorker};
use sp_api::ConstructRuntimeApi;
use sp_runtime::{
    traits::{BlakeTwo256, Block as BlockT},
    OpaqueExtrinsic,
};
use sp_trie::PrefixedMemoryDB;
use std::sync::Arc;

pub use client::*;

pub use sc_client_api::AuxStore;
use sc_consensus_babe::{self, SlotProportion};
pub use sp_blockchain::{HeaderBackend, HeaderMetadata};

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
    sc_consensus_grandpa::GrandpaBlockImport<
        FullBackend,
        Block,
        FullClient<RuntimeApi, ExecutorDispatch>,
        ChainSelection,
    >;

/// The transaction pool type defintion.
type TransactionPool<RuntimeApi, ExecutorDispatch> =
    sc_transaction_pool::FullPool<Block, FullClient<RuntimeApi, ExecutorDispatch>>;

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
        (
            impl Fn(
                crate::rpc::DenyUnsafe,
                sc_rpc::SubscriptionTaskExecutor,
            ) -> Result<jsonrpsee::RpcModule<()>, sc_service::Error>,
            (
                sc_consensus_babe::BabeBlockImport<
                    Block,
                    FullClient<RuntimeApi, ExecutorDispatch>,
                    FullGrandpaBlockImport<RuntimeApi, ExecutorDispatch>,
                >,
                sc_consensus_grandpa::LinkHalf<
                    Block,
                    FullClient<RuntimeApi, ExecutorDispatch>,
                    FullSelectChain,
                >,
                sc_consensus_babe::BabeLink<Block>,
            ),
            sc_consensus_grandpa::SharedVoterState,
            Option<Telemetry>,
        ),
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

    let (grandpa_block_import, grandpa_link) = sc_consensus_grandpa::block_import(
        client.clone(),
        &(client.clone() as Arc<_>),
        select_chain.clone(),
        telemetry.as_ref().map(|x| x.handle()),
    )?;
    let justification_import = grandpa_block_import.clone();

    let (block_import, babe_link) = sc_consensus_babe::block_import(
        sc_consensus_babe::configuration(&*client)?,
        grandpa_block_import,
        client.clone(),
    )?;

    let slot_duration = babe_link.config().slot_duration();
    let import_queue = sc_consensus_babe::import_queue(
        babe_link.clone(),
        block_import.clone(),
        Some(Box::new(justification_import)),
        client.clone(),
        select_chain.clone(),
        move |_, ()| async move {
            let timestamp = sp_timestamp::InherentDataProvider::from_system_time();

            let slot =
                sp_consensus_babe::inherents::InherentDataProvider::from_timestamp_and_slot_duration(
                    *timestamp,
                    slot_duration,
                );

            Ok((slot, timestamp))
        },
        &task_manager.spawn_essential_handle(),
        config.prometheus_registry(),
        telemetry.as_ref().map(|x| x.handle()),
    )?;

    let import_setup = (block_import, grandpa_link, babe_link);

    let (rpc_extensions_builder, rpc_setup) = {
        let (_, grandpa_link, babe_link) = &import_setup;

        let justification_stream = grandpa_link.justification_stream();
        let shared_authority_set = grandpa_link.shared_authority_set().clone();
        let shared_voter_state = sc_consensus_grandpa::SharedVoterState::empty();
        let shared_voter_state2 = shared_voter_state.clone();

        let finality_proof_provider = sc_consensus_grandpa::FinalityProofProvider::new_for_service(
            backend.clone(),
            Some(shared_authority_set.clone()),
        );

        let babe_config = babe_link.config().clone();
        let shared_epoch_changes = babe_link.epoch_changes().clone();

        let client = client.clone();
        let pool = transaction_pool.clone();
        let select_chain = select_chain.clone();
        let keystore = keystore_container.sync_keystore();
        let chain_spec = config.chain_spec.cloned_box();

        let rpc_backend = backend.clone();
        let rpc_extensions_builder = move |deny_unsafe, subscription_executor| {
            let deps = crate::rpc::FullDeps {
                client: client.clone(),
                pool: pool.clone(),
                select_chain: select_chain.clone(),
                chain_spec: chain_spec.cloned_box(),
                deny_unsafe,
                babe: crate::rpc::BabeDeps {
                    babe_config: babe_config.clone(),
                    shared_epoch_changes: shared_epoch_changes.clone(),
                    keystore: keystore.clone(),
                },
                grandpa: crate::rpc::GrandpaDeps {
                    shared_voter_state: shared_voter_state.clone(),
                    shared_authority_set: shared_authority_set.clone(),
                    justification_stream: justification_stream.clone(),
                    subscription_executor,
                    finality_provider: finality_proof_provider.clone(),
                },
            };

            crate::rpc::create_full(deps, rpc_backend.clone()).map_err(Into::into)
        };

        (rpc_extensions_builder, shared_voter_state2)
    };

    let partial = PartialComponents {
        client,
        backend,
        task_manager,
        keystore_container,
        select_chain,
        import_queue,
        transaction_pool,
        other: (rpc_extensions_builder, import_setup, rpc_setup, telemetry),
    };

    Ok(partial)
}

/// Result of [`new_full_base`].
pub struct NewFullBase<RuntimeApi, ExecutorDispatch>
where
    RuntimeApi: ConstructRuntimeApi<Block, FullClient<RuntimeApi, ExecutorDispatch>>
        + Send
        + Sync
        + 'static,
    RuntimeApi::RuntimeApi:
        RuntimeApiCollection<StateBackend = sc_client_api::StateBackendFor<FullBackend, Block>>,
    ExecutorDispatch: NativeExecutionDispatch + 'static,
{
    /// The task manager of the node.
    pub task_manager: TaskManager,
    /// The client instance of the node.
    pub client: Arc<FullClient<RuntimeApi, ExecutorDispatch>>,
    /// The networking service of the node.
    pub network: Arc<NetworkService<Block, <Block as BlockT>::Hash>>,
    /// The syncing service of the node.
    pub sync: Arc<SyncingService<Block>>,
    /// The transaction pool of the node.
    pub transaction_pool: Arc<TransactionPool<RuntimeApi, ExecutorDispatch>>,
    /// The rpc handlers of the node.
    pub rpc_handlers: RpcHandlers,
}

/// Creates a full service from the configuration.
pub fn new_full_base<RuntimeApi, ExecutorDispatch>(
    mut config: Configuration,
    disable_hardware_benchmarks: bool,
    with_startup_data: impl FnOnce(
        &sc_consensus_babe::BabeBlockImport<
            Block,
            FullClient<RuntimeApi, ExecutorDispatch>,
            FullGrandpaBlockImport<RuntimeApi, ExecutorDispatch>,
        >,
        &sc_consensus_babe::BabeLink<Block>,
    ),
) -> Result<NewFullBase<RuntimeApi, ExecutorDispatch>, ServiceError>
where
    RuntimeApi: ConstructRuntimeApi<Block, FullClient<RuntimeApi, ExecutorDispatch>>
        + Send
        + Sync
        + 'static,
    RuntimeApi::RuntimeApi:
        RuntimeApiCollection<StateBackend = sc_client_api::StateBackendFor<FullBackend, Block>>,
    ExecutorDispatch: NativeExecutionDispatch + 'static,
{
    let hwbench = (!disable_hardware_benchmarks)
        .then_some(config.database.path().map(|database_path| {
            let _ = std::fs::create_dir_all(database_path);
            sc_sysinfo::gather_hwbench(Some(database_path))
        }))
        .flatten();

    let sc_service::PartialComponents {
        client,
        backend,
        mut task_manager,
        import_queue,
        keystore_container,
        select_chain,
        transaction_pool,
        other: (rpc_builder, import_setup, rpc_setup, mut telemetry),
    } = new_partial(&config)?;

    let shared_voter_state = rpc_setup;
    let grandpa_protocol_name = sc_consensus_grandpa::protocol_standard_name(
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
        .push(sc_consensus_grandpa::grandpa_peers_set_config(
            grandpa_protocol_name.clone(),
        ));
    let warp_sync = Arc::new(sc_consensus_grandpa::warp_proof::NetworkProvider::new(
        backend.clone(),
        import_setup.1.shared_authority_set().clone(),
        Vec::default(),
    ));

    let (network, system_rpc_tx, tx_handler_controller, network_starter, sync_service) =
        sc_service::build_network(sc_service::BuildNetworkParams {
            config: &config,
            client: client.clone(),
            transaction_pool: transaction_pool.clone(),
            spawn_handle: task_manager.spawn_handle(),
            import_queue,
            block_announce_validator_builder: None,
            warp_sync_params: Some(WarpSyncParams::WithProvider(warp_sync)),
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
    let backoff_authoring_blocks =
        Some(sc_consensus_slots::BackoffAuthoringOnFinalizedHeadLagging::default());
    let name = config.network.node_name.clone();
    let enable_grandpa = !config.disable_grandpa;
    let prometheus_registry = config.prometheus_registry().cloned();

    let rpc_handlers = sc_service::spawn_tasks(sc_service::SpawnTasksParams {
        config,
        backend,
        client: client.clone(),
        keystore: keystore_container.sync_keystore(),
        network: network.clone(),
        rpc_builder: Box::new(rpc_builder),
        transaction_pool: transaction_pool.clone(),
        task_manager: &mut task_manager,
        system_rpc_tx,
        tx_handler_controller,
        sync_service: sync_service.clone(),
        telemetry: telemetry.as_mut(),
    })?;

    if let Some(hwbench) = hwbench {
        sc_sysinfo::print_hwbench(&hwbench);
        if !SUBSTRATE_REFERENCE_HARDWARE.check_hardware(&hwbench) && role.is_authority() {
            log::warn!(
                "⚠️  The hardware does not meet the minimal requirements for role 'Authority'."
            );
        }

        if let Some(ref mut telemetry) = telemetry {
            let telemetry_handle = telemetry.handle();
            task_manager.spawn_handle().spawn(
                "telemetry_hwbench",
                None,
                sc_sysinfo::initialize_hwbench_telemetry(telemetry_handle, hwbench),
            );
        }
    }

    let (block_import, grandpa_link, babe_link) = import_setup;

    (with_startup_data)(&block_import, &babe_link);

    if let sc_service::config::Role::Authority { .. } = &role {
        let proposer = authorship::ProposerFactory::new(
            task_manager.spawn_handle(),
            client.clone(),
            transaction_pool.clone(),
            prometheus_registry.as_ref(),
            telemetry.as_ref().map(|x| x.handle()),
        );

        let client_clone = client.clone();
        let slot_duration = babe_link.config().slot_duration();
        let babe_config = sc_consensus_babe::BabeParams {
            keystore: keystore_container.sync_keystore(),
            client: client.clone(),
            select_chain,
            env: proposer,
            block_import,
            sync_oracle: sync_service.clone(),
            justification_sync_link: sync_service.clone(),
            create_inherent_data_providers: move |parent, ()| {
                let client_clone = client_clone.clone();
                async move {
                    let timestamp = sp_timestamp::InherentDataProvider::from_system_time();

                    let slot =
                        sp_consensus_babe::inherents::InherentDataProvider::from_timestamp_and_slot_duration(
                            *timestamp,
                            slot_duration,
                        );

                    let storage_proof =
                        sp_transaction_storage_proof::registration::new_data_provider(
                            &*client_clone,
                            &parent,
                        )?;

                    Ok((slot, timestamp, storage_proof))
                }
            },
            force_authoring,
            backoff_authoring_blocks,
            babe_link,
            block_proposal_slot_portion: SlotProportion::new(0.5),
            max_block_proposal_slot_portion: None,
            telemetry: telemetry.as_ref().map(|x| x.handle()),
        };

        let babe = sc_consensus_babe::start_babe(babe_config)?;
        task_manager.spawn_essential_handle().spawn_blocking(
            "babe-proposer",
            Some("block-authoring"),
            babe,
        );
    }

    // if the node isn't actively participating in consensus then it doesn't
    // need a keystore, regardless of which protocol we use below.
    let keystore = if role.is_authority() {
        Some(keystore_container.sync_keystore())
    } else {
        None
    };

    let config = sc_consensus_grandpa::Config {
        // FIXME #1578 make this available through chainspec
        gossip_duration: std::time::Duration::from_millis(333),
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
        let grandpa_config = sc_consensus_grandpa::GrandpaParams {
            config,
            link: grandpa_link,
            network: network.clone(),
            sync: Arc::new(sync_service.clone()),
            telemetry: telemetry.as_ref().map(|x| x.handle()),
            voting_rule: sc_consensus_grandpa::VotingRulesBuilder::default().build(),
            prometheus_registry,
            shared_voter_state,
        };

        // the GRANDPA voter task is considered infallible, i.e.
        // if it fails we take down the service with it.
        task_manager.spawn_essential_handle().spawn_blocking(
            "grandpa-voter",
            None,
            sc_consensus_grandpa::run_grandpa_voter(grandpa_config)?,
        );
    }

    network_starter.start_network();
    Ok(NewFullBase {
        task_manager,
        client,
        network,
        sync: sync_service,
        transaction_pool,
        rpc_handlers,
    })
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
        sc_consensus_grandpa::revert(client, self.blocks)?;
        Ok(())
    }
}

/// Build a full node for different chain "flavors".
///
/// The actual "flavor", aka if it will use `Gear`, `Vara` etc. is determined based on
/// [`IdentifyVariant`] using the chain spec.
pub fn new_full(
    config: Configuration,
    disable_hardware_benchmarks: bool,
) -> Result<TaskManager, ServiceError> {
    match &config.chain_spec {
        #[cfg(feature = "gear-native")]
        spec if spec.is_gear() => new_full_base::<gear_runtime::RuntimeApi, GearExecutorDispatch>(
            config,
            disable_hardware_benchmarks,
            |_, _| (),
        )
        .map(|NewFullBase { task_manager, .. }| task_manager),
        #[cfg(feature = "vara-native")]
        spec if spec.is_vara() => new_full_base::<vara_runtime::RuntimeApi, VaraExecutorDispatch>(
            config,
            disable_hardware_benchmarks,
            |_, _| (),
        )
        .map(|NewFullBase { task_manager, .. }| task_manager),
        _ => Err(ServiceError::Other("Invalid chain spec".into())),
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
