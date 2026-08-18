// Copyright (C) Parity Technologies (UK) Ltd.
// SPDX-License-Identifier: GPL-3.0-or-later WITH Classpath-exception-2.0

use clap::Args;
use std::str::FromStr;

/// Parameters used to config runtime.
#[derive(Debug, Clone, Args)]
pub struct RuntimeParams {
    /// The size of the instances cache for each runtime [max: 32].
    ///
    /// Values higher than 32 are illegal.
    #[arg(long, default_value_t = 8, value_parser = parse_max_runtime_instances)]
    pub max_runtime_instances: usize,

    /// Maximum number of different runtimes that can be cached.
    #[arg(long, default_value_t = 2)]
    pub runtime_cache_size: u8,
}

fn parse_max_runtime_instances(s: &str) -> Result<usize, String> {
    let max_runtime_instances = usize::from_str(s)
        .map_err(|_err| format!("Illegal `--max-runtime-instances` value: {s}"))?;

    if max_runtime_instances > 32 {
        Err(format!("Illegal `--max-runtime-instances` value: {max_runtime_instances} is more than the allowed maximum of `32` "))
    } else {
        Ok(max_runtime_instances)
    }
}
