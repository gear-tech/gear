// Copyright (C) Gear Technologies Inc.
// SPDX-License-Identifier: GPL-3.0-or-later WITH Classpath-exception-2.0

use clap::Parser;
use std::str::FromStr;

#[allow(missing_docs)]
#[derive(Debug, Clone, Parser, derive_more::Display)]
pub enum SandboxBackend {
    #[display("wasmtime")]
    Wasmtime,
    #[display("wasmi")]
    Wasmi,
}

// TODO: use `derive_more::FromStr` when derive_more dependency is updated to 1.0
impl FromStr for SandboxBackend {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "wasmtime" => Ok(SandboxBackend::Wasmtime),
            "wasmi" => Ok(SandboxBackend::Wasmi),
            _ => Err(format!("Unknown sandbox executor: {s}")),
        }
    }
}

#[allow(missing_docs)]
#[derive(Debug, Parser)]
#[group(skip)]
pub struct RunCmd {
    #[allow(missing_docs)]
    #[command(flatten)]
    pub base: sc_cli::RunCmd,

    /// The Wasm host executor to use in program sandbox.
    #[arg(long, default_value_t = SandboxBackend::Wasmtime)]
    pub sandbox_backend: SandboxBackend,

    /// Sets a limit at which the underlying sandbox store will be cleared (applies only to the Wasmtime sandbox backend),
    /// potentially altering performance characteristics.
    // TODO: remove clear counter <https://github.com/gear-tech/gear/issues/5465>
    #[arg(long, default_value_t = 50)]
    pub sandbox_store_clear_counter_limit: u32,

    /// The upper limit for the amount of gas a validator can burn in one block.
    #[arg(long)]
    pub max_gas: Option<u64>,

    /// The upper limit for the amount of gas a runtime api can burn in one call.
    #[arg(long, default_value_t = 64)]
    pub rpc_calculations_multiplier: u64,

    /// The upper limit for the amount of calls in rpc batch.
    #[arg(long, default_value_t = 256)]
    pub rpc_max_batch_size: u64,
}

#[derive(Debug, Parser)]
pub struct Cli {
    #[command(subcommand)]
    pub subcommand: Option<Subcommand>,

    #[command(flatten)]
    pub run: RunCmd,

    /// Disable automatic hardware benchmarks.
    ///
    /// By default these benchmarks are automatically ran at startup and measure
    /// the CPU speed, the memory bandwidth and the disk speed.
    ///
    /// The results are then printed out in the logs, and also sent as part of
    /// telemetry, if telemetry is enabled.
    #[arg(long)]
    pub no_hardware_benchmarks: bool,
}

#[allow(clippy::large_enum_variant)]
#[derive(Debug, clap::Subcommand)]
pub enum Subcommand {
    /// Key management cli utilities
    #[command(subcommand)]
    Key(sc_cli::KeySubcommand),

    /// Build a chain specification.
    BuildSpec(sc_cli::BuildSpecCmd),

    /// Validate blocks.
    CheckBlock(sc_cli::CheckBlockCmd),

    /// Export blocks.
    ExportBlocks(sc_cli::ExportBlocksCmd),

    /// Export the state of a given block into a chain spec.
    ExportState(sc_cli::ExportStateCmd),

    /// Import blocks.
    ImportBlocks(sc_cli::ImportBlocksCmd),

    /// Remove the whole chain.
    PurgeChain(sc_cli::PurgeChainCmd),

    /// Revert the chain to a previous state.
    Revert(sc_cli::RevertCmd),

    /// Sub-commands concerned with benchmarking.
    #[cfg(feature = "runtime-benchmarks")]
    #[command(subcommand)]
    Benchmark(frame_benchmarking_cli::BenchmarkCmd),

    /// Try-runtime has migrated to a standalone CLI
    /// (<https://github.com/paritytech/try-runtime-cli>). The subcommand exists as a stub and
    /// deprecation notice. It will be removed entirely some time after January 2024.
    TryRuntime,

    /// Db meta columns information.
    ChainInfo(sc_cli::ChainInfoCmd),

    /// Program CLI
    ///
    /// # NOTE
    ///
    /// Only support gear runtime when features include both `gear-program/gear`
    /// and `gear-program/vara`.
    #[cfg(feature = "cli")]
    #[command(name = "gcli", about = "Run gear program cli.")]
    Cli(gcli::Cli),
}
