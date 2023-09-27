// This file is part of Gear.

// Copyright (C) 2021-2023 Gear Technologies Inc.
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

use crate::cli::{Cli, Subcommand};
use runtime_primitives::Block;
use sc_cli::{ChainSpec, ExecutionStrategy, RuntimeVersion, SubstrateCli};
use sc_service::config::BasePath;
use service::{chain_spec, IdentifyVariant};

#[cfg(feature = "try-runtime")]
use try_runtime_cli::block_building_info::substrate_info;

impl SubstrateCli for Cli {
    fn impl_name() -> String {
        "Gear Node".into()
    }

    fn impl_version() -> String {
        env!("SUBSTRATE_CLI_IMPL_VERSION").into()
    }

    fn description() -> String {
        env!("CARGO_PKG_DESCRIPTION").into()
    }

    fn author() -> String {
        env!("CARGO_PKG_AUTHORS").into()
    }

    fn support_url() -> String {
        "gear-tech.io".into()
    }

    fn copyright_start_year() -> i32 {
        2021
    }

    fn load_spec(&self, id: &str) -> Result<Box<dyn sc_service::ChainSpec>, String> {
        Ok(match id {
            #[cfg(not(feature = "dev"))]
            "dev" | "gear-dev" | "vara-dev" => return Err("Development runtimes are not available. Please compile the node with `-F dev` to enable it.".into()),
            #[cfg(all(feature = "gear-native", feature = "dev"))]
            "dev" | "gear-dev" => Box::new(chain_spec::gear::development_config()?),
            #[cfg(all(feature = "vara-native", feature = "dev"))]
            "vara-dev" => Box::new(chain_spec::vara::development_config()?),
            #[cfg(feature = "gear-native")]
            "local" | "gear-local" => {
                #[cfg(feature = "dev")]
                log::warn!("Running `gear-local` in `dev` mode");
                Box::new(chain_spec::gear::local_testnet_config()?)
            }
            #[cfg(feature = "vara-native")]
            "vara" => Box::new(chain_spec::RawChainSpec::from_json_bytes(
                &include_bytes!("../../res/vara.json")[..],
            )?),
            #[cfg(feature = "vara-native")]
            "vara-local" => {
                #[cfg(feature = "dev")]
                log::warn!("Running `vara-local` in `dev` mode");
                Box::new(chain_spec::vara::local_testnet_config()?)
            }
            #[cfg(feature = "gear-native")]
            "staging" | "gear-staging" => {
                #[cfg(feature = "dev")]
                log::warn!("Running `gear-staging` in `dev` mode");
                Box::new(chain_spec::gear::staging_testnet_config()?)
            }
            "test" | "" => Box::new(chain_spec::RawChainSpec::from_json_bytes(
                &include_bytes!("../../res/staging.json")[..],
            )?),
            path => {
                let path = std::path::PathBuf::from(path);

                let chain_spec = Box::new(chain_spec::RawChainSpec::from_json_file(path.clone())?)
                    as Box<dyn ChainSpec>;

                if chain_spec.is_dev() {
                    #[cfg(not(feature = "dev"))]
                    return Err("Development runtimes are not available. Please compile the node with `-F dev` to enable it.".into());
                }

                // When `force_*` is provide or the file name starts with the name of a known chain,
                // we use the chain spec for the specific chain.
                if self.run.force_vara || chain_spec.is_vara() {
                    #[cfg(feature = "vara-native")]
                    return Ok(Box::new(chain_spec::vara::ChainSpec::from_json_file(path)?));

                    #[cfg(not(feature = "vara-native"))]
                    return Err("Vara runtime is not available. Please compile the node with `-F vara-native` to enable it.".into());
                } else {
                    #[cfg(feature = "gear-native")]
                    return Ok(Box::new(chain_spec::gear::ChainSpec::from_json_file(path)?));

                    #[cfg(not(feature = "gear-native"))]
                    return Err("Gear runtime is not available. Please compile the node with `-F gear-native` to enable it.".into());
                }
            }
        })
    }

    fn native_runtime_version(spec: &Box<dyn ChainSpec>) -> &'static RuntimeVersion {
        match spec {
            #[cfg(feature = "gear-native")]
            spec if spec.is_gear() => &service::gear_runtime::VERSION,
            #[cfg(feature = "vara-native")]
            spec if spec.is_vara() => &service::vara_runtime::VERSION,
            _ => panic!("Invalid chain spec"),
        }
    }
}

/// Unwraps a [`service::Client`] into the concrete runtime client.
#[allow(unused)]
macro_rules! unwrap_client {
    (
        $client:ident,
        $code:expr
    ) => {
        match $client.as_ref() {
            #[cfg(feature = "gear-native")]
            service::Client::Gear($client) => $code,
            #[cfg(feature = "vara-native")]
            service::Client::Vara($client) => $code,
            #[allow(unreachable_patterns)]
            _ => Err("invalid chain spec".into()),
        }
    };
}

/// Parse and run command line arguments
pub fn run() -> sc_cli::Result<()> {
    let mut cli = Cli::from_args();

    let old_base = BasePath::from_project("", "", "gear-node");
    let new_base = BasePath::from_project("", "", &Cli::executable_name());
    if old_base.path().exists() && !new_base.path().exists() {
        _ = std::fs::rename(old_base.path(), new_base.path());
    }

    let base = &mut cli.run.base;

    // Force setting `Wasm` as default execution strategy.
    let execution_strategy = base
        .import_params
        .execution_strategies
        .execution
        .get_or_insert(ExecutionStrategy::Wasm);

    // Checking if node supposed to be validator (explicitly or by shortcuts).
    let is_validator = base.validator
        || base.shared_params.dev
        || base.alice
        || base.bob
        || base.charlie
        || base.dave
        || base.eve
        || base.ferdie
        || base.one
        || base.two;

    // Denying ability to validate blocks with non-wasm execution.
    if is_validator && *execution_strategy != ExecutionStrategy::Wasm {
        return Err(
            "Node can be --validator only with wasm execution strategy. To enable it run the node with `--execution wasm` or without the flag for default value."
                .into(),
        );
    }

    match &cli.subcommand {
        Some(Subcommand::Key(cmd)) => cmd.run(&cli),
        Some(Subcommand::BuildSpec(cmd)) => {
            let runner = cli.create_runner(cmd)?;
            runner.sync_run(|config| cmd.run(config.chain_spec, config.network))
        }
        Some(Subcommand::CheckBlock(cmd)) => {
            let runner = cli.create_runner(cmd)?;
            runner.async_run(|config| {
                let (client, _, import_queue, task_manager) = service::new_chain_ops(
                    &config,
                    cli.run.rpc_calculations_multiplier,
                    cli.run.rpc_max_batch_size,
                )?;
                Ok((cmd.run(client, import_queue), task_manager))
            })
        }
        Some(Subcommand::ExportBlocks(cmd)) => {
            let runner = cli.create_runner(cmd)?;
            runner.async_run(|config| {
                let (client, _, _, task_manager) = service::new_chain_ops(
                    &config,
                    cli.run.rpc_calculations_multiplier,
                    cli.run.rpc_max_batch_size,
                )?;
                Ok((cmd.run(client, config.database), task_manager))
            })
        }
        Some(Subcommand::ExportState(cmd)) => {
            let runner = cli.create_runner(cmd)?;
            runner.async_run(|config| {
                let (client, _, _, task_manager) = service::new_chain_ops(
                    &config,
                    cli.run.rpc_calculations_multiplier,
                    cli.run.rpc_max_batch_size,
                )?;
                Ok((cmd.run(client, config.chain_spec), task_manager))
            })
        }
        Some(Subcommand::ImportBlocks(cmd)) => {
            let runner = cli.create_runner(cmd)?;
            runner.async_run(|config| {
                let (client, _, import_queue, task_manager) = service::new_chain_ops(
                    &config,
                    cli.run.rpc_calculations_multiplier,
                    cli.run.rpc_max_batch_size,
                )?;
                Ok((cmd.run(client, import_queue), task_manager))
            })
        }
        Some(Subcommand::PurgeChain(cmd)) => {
            let runner = cli.create_runner(cmd)?;
            runner.sync_run(|config| cmd.run(config.database))
        }
        Some(Subcommand::Revert(cmd)) => {
            let runner = cli.create_runner(cmd)?;
            runner.async_run(|config| {
                let (client, backend, _, task_manager) = service::new_chain_ops(
                    &config,
                    cli.run.rpc_calculations_multiplier,
                    cli.run.rpc_max_batch_size,
                )?;
                let aux_revert = Box::new(|client, backend, blocks| {
                    service::revert_backend(client, backend, blocks, config)
                        .map_err(|err| sc_cli::Error::Application(err.into()))
                });
                Ok((cmd.run(client, backend, Some(aux_revert)), task_manager))
            })
        }
        #[cfg(feature = "runtime-benchmarks")]
        Some(Subcommand::Benchmark(cmd)) => {
            use crate::{inherent_benchmark_data, RemarkBuilder, TransferKeepAliveBuilder};
            use frame_benchmarking_cli::{
                BenchmarkCmd, ExtrinsicFactory, SUBSTRATE_REFERENCE_HARDWARE,
            };
            use sp_keyring::Sr25519Keyring;

            let runner = cli.create_runner(cmd)?;

            runner.sync_run(|config| {
                // This switch needs to be in the client, since the client decides
                // which sub-commands it wants to support.
                match cmd {
                    BenchmarkCmd::Pallet(cmd) => {
                        if !cfg!(feature = "runtime-benchmarks") {
                            return Err(
                                "Runtime benchmarking wasn't enabled when building the node. \
                            You can enable it with `--features runtime-benchmarks`."
                                    .into(),
                            );
                        }
                        match &config.chain_spec {
                            #[cfg(feature = "gear-native")]
                            spec if spec.is_gear() => cmd
                                .run::<service::gear_runtime::Block, service::GearExecutorDispatch>(
                                    config,
                                ),
                            #[cfg(feature = "vara-native")]
                            spec if spec.is_vara() => cmd
                                .run::<service::vara_runtime::Block, service::VaraExecutorDispatch>(
                                    config,
                                ),
                            _ => Err("invalid chain spec".into()),
                        }
                    }
                    BenchmarkCmd::Block(cmd) => {
                        let (client, _, _, _) = service::new_chain_ops(
                            &config,
                            cli.run.rpc_calculations_multiplier,
                            cli.run.rpc_max_batch_size,
                        )?;

                        unwrap_client!(client, cmd.run(client.clone()))
                    }
                    #[cfg(not(feature = "runtime-benchmarks"))]
                    BenchmarkCmd::Storage(_) => Err(
                        "Storage benchmarking can be enabled with `--features runtime-benchmarks`."
                            .into(),
                    ),
                    #[cfg(feature = "runtime-benchmarks")]
                    BenchmarkCmd::Storage(cmd) => {
                        let (client, backend, _, _) = service::new_chain_ops(
                            &config,
                            cli.run.rpc_calculations_multiplier,
                            cli.run.rpc_max_batch_size,
                        )?;
                        let db = backend.expose_db();
                        let storage = backend.expose_storage();

                        unwrap_client!(client, cmd.run(config, client.clone(), db, storage))
                    }
                    BenchmarkCmd::Overhead(cmd) => {
                        let inherent_data = inherent_benchmark_data().map_err(|e| {
                            sc_cli::Error::from(format!("generating inherent data: {e:?}"))
                        })?;

                        let (client, _, _, _) = service::new_chain_ops(
                            &config,
                            cli.run.rpc_calculations_multiplier,
                            cli.run.rpc_max_batch_size,
                        )?;
                        let ext_builder = RemarkBuilder::new(client.clone());

                        unwrap_client!(
                            client,
                            cmd.run(
                                config,
                                client.clone(),
                                inherent_data,
                                Vec::new(),
                                &ext_builder
                            )
                        )
                    }
                    BenchmarkCmd::Extrinsic(cmd) => {
                        let inherent_data = inherent_benchmark_data().map_err(|e| {
                            sc_cli::Error::from(format!("generating inherent data: {e:?}"))
                        })?;
                        let (client, _, _, _) = service::new_chain_ops(
                            &config,
                            cli.run.rpc_calculations_multiplier,
                            cli.run.rpc_max_batch_size,
                        )?;
                        // Register the *Remark* and *TKA* builders.
                        let ext_factory = ExtrinsicFactory(vec![
                            Box::new(RemarkBuilder::new(client.clone())),
                            Box::new(TransferKeepAliveBuilder::new(
                                client.clone(),
                                Sr25519Keyring::Alice.to_account_id(),
                            )),
                        ]);

                        unwrap_client!(
                            client,
                            cmd.run(client.clone(), inherent_data, Vec::new(), &ext_factory)
                        )
                    }
                    BenchmarkCmd::Machine(cmd) => {
                        cmd.run(&config, SUBSTRATE_REFERENCE_HARDWARE.clone())
                    }
                }
            })
        }
        #[cfg(feature = "try-runtime")]
        Some(Subcommand::TryRuntime(cmd)) => {
            use sc_executor::{sp_wasm_interface::ExtendedHostFunctions, NativeExecutionDispatch};
            let runner = cli.create_runner(cmd)?;
            let chain_spec = &runner.config().chain_spec;

            let registry = &runner
                .config()
                .prometheus_config
                .as_ref()
                .map(|cfg| &cfg.registry);
            let task_manager =
                sc_service::TaskManager::new(runner.config().tokio_handle.clone(), *registry)
                    .map_err(|e| sc_cli::Error::Service(sc_service::Error::Prometheus(e)))?;

            match chain_spec {
                #[cfg(feature = "gear-native")]
                spec if spec.is_gear() => runner.async_run(|_| {
                    let info_provider =
                        substrate_info(gear_runtime::constants::time::SLOT_DURATION);
                    Ok((
                        cmd.run::<service::gear_runtime::Block, ExtendedHostFunctions<
						sp_io::SubstrateHostFunctions,
						<service::GearExecutorDispatch as NativeExecutionDispatch>::ExtendHostFunctions,
					>, _>(Some(info_provider)),
                        task_manager,
                    ))
                }),
                #[cfg(feature = "vara-native")]
                spec if spec.is_vara() => runner.async_run(|_| {
                    let info_provider =
                        substrate_info(vara_runtime::constants::time::SLOT_DURATION);
                    Ok((
                        cmd.run::<service::vara_runtime::Block, ExtendedHostFunctions<
						sp_io::SubstrateHostFunctions,
						<service::VaraExecutorDispatch as NativeExecutionDispatch>::ExtendHostFunctions,
					>, _>(Some(info_provider)),
                        task_manager,
                    ))
                }),
                _ => panic!("No runtime feature [gear, vara] is enabled"),
            }
        }
        #[cfg(not(feature = "try-runtime"))]
        Some(Subcommand::TryRuntime) => Err("TryRuntime wasn't enabled when building the node. \
                You can enable it with `--features try-runtime`."
            .into()),
        Some(Subcommand::ChainInfo(cmd)) => {
            let runner = cli.create_runner(cmd)?;
            runner.sync_run(|config| cmd.run::<Block>(&config))
        }
        #[cfg(feature = "cli")]
        Some(Subcommand::Cli(gp)) => {
            // # NOTE
            //
            // unwrap here directly to show the error messages.
            gp.exec_sync().unwrap();
            Ok(())
        }
        None => {
            let runner = if cli.run.base.validator && cli.run.base.shared_params.log.is_empty() {
                cli.create_runner_with_logger_hook(&cli.run.base, |logger, _| {
                    logger.with_detailed_output(false);
                    logger.with_max_level(log::LevelFilter::Info);
                })?
            } else {
                cli.create_runner(&cli.run.base)?
            };

            runner.run_node_until_exit(|config| async move {
                service::new_full(
                    config,
                    cli.no_hardware_benchmarks,
                    cli.run.max_gas,
                    cli.run.rpc_calculations_multiplier,
                    cli.run.rpc_max_batch_size,
                )
                .map_err(sc_cli::Error::Service)
            })
        }
    }
}
