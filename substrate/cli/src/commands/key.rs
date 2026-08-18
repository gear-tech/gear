// Copyright (C) Parity Technologies (UK) Ltd.
// SPDX-License-Identifier: GPL-3.0-or-later WITH Classpath-exception-2.0

//! Key related CLI utilities

use super::{
    generate::GenerateCmd, generate_node_key::GenerateNodeKeyCmd, insert_key::InsertKeyCmd,
    inspect_key::InspectKeyCmd, inspect_node_key::InspectNodeKeyCmd,
};
use crate::{Error, SubstrateCli};

/// Key utilities for the cli.
#[derive(Debug, clap::Subcommand)]
pub enum KeySubcommand {
    /// Generate a random node key, write it to a file or stdout and write the
    /// corresponding peer-id to stderr
    GenerateNodeKey(GenerateNodeKeyCmd),

    /// Generate a random account
    Generate(GenerateCmd),

    /// Gets a public key and a SS58 address from the provided Secret URI
    Inspect(InspectKeyCmd),

    /// Load a node key from a file or stdin and print the corresponding peer-id
    InspectNodeKey(InspectNodeKeyCmd),

    /// Insert a key to the keystore of a node.
    Insert(InsertKeyCmd),
}

impl KeySubcommand {
    /// run the key subcommands
    pub fn run<C: SubstrateCli>(&self, cli: &C) -> Result<(), Error> {
        match self {
            KeySubcommand::GenerateNodeKey(cmd) => {
                let chain_spec = cli.load_spec(cmd.chain.as_deref().unwrap_or(""))?;
                cmd.run(chain_spec.id(), &C::executable_name())
            }
            KeySubcommand::Generate(cmd) => cmd.run(),
            KeySubcommand::Inspect(cmd) => cmd.run(),
            KeySubcommand::Insert(cmd) => cmd.run(cli),
            KeySubcommand::InspectNodeKey(cmd) => cmd.run(),
        }
    }
}
