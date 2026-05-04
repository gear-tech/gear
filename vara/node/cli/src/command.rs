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

use crate::{
    SandboxBackend,
    cli::{Cli, Subcommand},
};
use runtime_primitives::Block;
use sc_cli::{ChainSpec, SubstrateCli};
use sc_service::config::BasePath;
use service::{IdentifyVariant, chain_spec};

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
            // Common "dev" chain. `vara-runtime` is prioritized.
            #[cfg(feature = "vara-native")]
            "dev" => Box::new(chain_spec::vara::development_config()?),
            #[cfg(not(feature = "vara-native"))]
            "dev" => return Err("No runtimes specified to compile."),

            // Specific "dev" chains.
            #[cfg(feature = "vara-native")]
            "vara-dev" => Box::new(chain_spec::vara::development_config()?),

            // Common "local" chain. `vara-runtime` is prioritized.
            #[cfg(feature = "vara-native")]
            "local" => Box::new(chain_spec::vara::local_testnet_config()?),
            #[cfg(not(feature = "vara-native"))]
            "local" => return Err("No runtimes specified to compile."),

            // Specific "local" chains.
            #[cfg(feature = "vara-native")]
            "vara-local" => Box::new(chain_spec::vara::local_testnet_config()?),

            // Production chains.
            "vara" => Box::new(chain_spec::RawChainSpec::from_json_bytes(
                &include_bytes!("../../res/vara.json")[..],
            )?),

            // Common "testnet" chain. `vara-runtime` is prioritized (currently the only available).
            "testnet" => Box::new(chain_spec::RawChainSpec::from_json_bytes(
                &include_bytes!("../../res/vara_testnet.json")[..],
            )?),

            // Specific "testnet" chains.
            "vara-testnet" => Box::new(chain_spec::RawChainSpec::from_json_bytes(
                &include_bytes!("../../res/vara_testnet.json")[..],
            )?),

            // Empty (default) chain.
            "" => Box::new(chain_spec::RawChainSpec::from_json_bytes(
                &include_bytes!("../../res/vara_testnet.json")[..],
            )?),

            // Custom chain spec.
            path => {
                let path = std::path::PathBuf::from(path);

                let chain_spec = Box::new(chain_spec::RawChainSpec::from_json_file(path.clone())?)
                    as Box<dyn ChainSpec>;

                if chain_spec.is_vara() {
                    // Vara specs.
                    #[cfg(feature = "vara-runtime")]
                    return Ok(Box::new(chain_spec::vara::ChainSpec::from_json_file(path)?));
                    #[cfg(not(feature = "vara-runtime"))]
                    return Err("Vara runtime is not available. Please compile the node with `-F vara-native` to enable it.".into());
                } else {
                    return Err("Unable to identify chain spec as vara runtime".into());
                }
            }
        })
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
            #[cfg(feature = "vara-native")]
            service::Client::Vara($client) => $code,
            #[allow(unreachable_patterns)]
            _ => Err("invalid chain spec".into()),
        }
    };
}

/// Parse and run command line arguments
#[allow(clippy::result_large_err)]
pub fn run() -> sc_cli::Result<()> {
    let cli = Cli::from_args();

    gear_runtime_interface::sandbox_init(
        match cli.run.sandbox_backend {
            SandboxBackend::Wasmer => gear_runtime_interface::SandboxBackend::Wasmer,
            SandboxBackend::Wasmi => gear_runtime_interface::SandboxBackend::Wasmi,
        },
        cli.run.sandbox_store_clear_counter_limit.into(),
    );

    let old_base = BasePath::from_project("", "", "gear-node");
    let new_base = BasePath::from_project("", "", &Cli::executable_name());
    if old_base.path().exists() && !new_base.path().exists() {
        _ = std::fs::rename(old_base.path(), new_base.path());
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
            use crate::{RemarkBuilder, TransferKeepAliveBuilder, inherent_benchmark_data};
            use frame_benchmarking_cli::{
                BenchmarkCmd, ExtrinsicFactory, SUBSTRATE_REFERENCE_HARDWARE,
            };
            use sc_executor::sp_wasm_interface::ExtendedHostFunctions;
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
                            #[cfg(feature = "vara-native")]
                            spec if spec.is_vara() => cmd
                                .run_with_spec::<sp_runtime::traits::HashingFor<service::vara_runtime::Block>, ExtendedHostFunctions<
                                    sp_io::SubstrateHostFunctions,
                                    service::ExtendHostFunctions,
                                >>(Some(config.chain_spec)),
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
        Some(Subcommand::ChainInfo(cmd)) => {
            let runner = cli.create_runner(cmd)?;
            runner.sync_run(|config| cmd.run::<Block>(&config))
        }
        #[cfg(feature = "cli")]
        Some(Subcommand::Cli(gp)) => {
            // # NOTE
            //
            // unwrap here directly to show the error messages.
            gp.clone().run_blocking().unwrap();
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
        _ => Err("Unknown command.".into()),
    }
}
