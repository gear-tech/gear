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

use clap::Parser;
use gear_wasm_builder::optimize::{OptType, Optimizer};
use std::{fs, path::PathBuf};

#[derive(Debug, thiserror::Error)]
enum Error {
    #[error("Multiple skipping functional")]
    InvalidSkip,
}

#[derive(Debug, clap::Parser)]
struct Args {
    #[clap(short, long, value_parser, multiple = true)]
    path: Vec<String>,
    #[clap(long)]
    skip_meta: bool,
    #[clap(long)]
    skip_opt: bool,
    #[clap(long)]
    skip_stack_end: bool,
    #[clap(long)]
    skip_stripping_custom_sections: bool,
    #[clap(short, long)]
    verbose: bool,
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let Args {
        path: wasm_files,
        skip_meta,
        skip_opt,
        skip_stack_end,
        skip_stripping_custom_sections,
        verbose,
    } = Args::parse();

    let mut env = env_logger::Env::default();
    if verbose {
        env = env.default_filter_or("debug");
    }
    env_logger::Builder::from_env(env).init();

    if skip_meta && skip_opt {
        return Err(Box::new(Error::InvalidSkip));
    }

    for file in &wasm_files {
        if !file.ends_with(".wasm") || file.ends_with(".meta.wasm") || file.ends_with(".opt.wasm") {
            continue;
        }

        let file = PathBuf::from(file);
        let res = gear_wasm_builder::optimize::optimize_wasm(file.clone(), "s", true)?;

        log::info!(
            "wasm-opt: {} {} Kb -> {} Kb",
            res.dest_wasm.display(),
            res.original_size,
            res.optimized_size
        );

        let mut optimizer = Optimizer::new(file.clone())?;

        if !skip_stack_end {
            optimizer.insert_stack_and_export();
        }

        if !skip_stripping_custom_sections {
            optimizer.strip_custom_sections();
        }

        if !skip_opt {
            let path = file.with_extension("opt.wasm");

            log::debug!("*** Processing chain optimization: {}", path.display());
            let code = optimizer.optimize(OptType::Opt)?;
            log::debug!("Optimized wasm: {}", path.to_string_lossy());

            fs::write(path, code)?;
        }

        if !skip_meta {
            let path = file.with_extension("meta.wasm");

            log::debug!("*** Processing metadata optimization: {}", path.display());
            let code = optimizer.optimize(OptType::Meta)?;
            log::debug!("Metadata wasm: {}", path.to_string_lossy());

            fs::write(path, code)?;
        }
    }

    Ok(())
}
