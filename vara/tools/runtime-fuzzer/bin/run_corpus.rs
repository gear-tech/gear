// Copyright (C) Gear Technologies Inc.
// SPDX-License-Identifier: GPL-3.0-or-later WITH Classpath-exception-2.0

//! Runs provided from the cli corpus
//!
//! Alternatively, `cargo fuzz run` can be used to reproduce some corpus,
//! but it won't give logs of [`GearCalls`](gear_call_gen::GearCall) generation, which sheds some
//! light on how `gear-wasm-gen` worked.
//!
//! Also that script can be used to run any bytes input, not only fuzzer's
//! corpus.
//!
//! Just simply run `cargo run --release -- -p <path_to_corpus>`.

use anyhow::Result;
use clap::Parser;
use gear_wasm_gen::wasm_gen_arbitrary::{Arbitrary, Unstructured};
use runtime_fuzzer::{self, FuzzerInput};
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
    let fuzzer_input = {
        let mut u = Unstructured::new(&corpus_bytes);
        FuzzerInput::arbitrary(&mut u)?
    };

    gear_utils::init_default_logger();

    runtime_fuzzer::run(fuzzer_input).expect("Fuzzer run failed");

    Ok(())
}
