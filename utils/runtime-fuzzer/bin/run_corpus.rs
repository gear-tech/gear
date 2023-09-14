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

//! Runs provided from the cli corpus
//!
//! Alternatively, `cargo fuzz run` can be used to reproduce some corpus,
//! but it won't give logs of [`GearCalls`] generation, which sheds some
//! light on how `gear-wasm-gen` worked.
//!
//! Also that script can be used to run any bytes input, not only fuzzer's
//! corpus.
//!
//! Just simply run `cargo run --release -- -p <path_to_corpus>`.

use anyhow::Result;
use arbitrary::{Arbitrary, Unstructured};
use clap::Parser;
use runtime_fuzzer::{self, GearCalls};
use std::{fs, path::PathBuf};

/// A simple tool to run corpus.
#[derive(Debug, Parser)]
#[command(author, version, about, long_about = None)]
struct Params {
    /// Path to the file, which contains corpus.
    #[arg(short, long)]
    path: PathBuf,
}

fn main() -> Result<()> {
    let params = Params::parse();

    let corpus_bytes = fs::read(params.path)?;

    gear_utils::init_default_logger();

    let mut unstructured = Unstructured::new(&corpus_bytes);
    let gear_calls = GearCalls::arbitrary(&mut unstructured)?;

    runtime_fuzzer::run(gear_calls);

    Ok(())
}
