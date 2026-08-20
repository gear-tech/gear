// Copyright (C) Parity Technologies (UK) Ltd.
// SPDX-License-Identifier: GPL-3.0-or-later WITH Classpath-exception-2.0

use crate::{
    error,
    params::{DatabaseParams, SharedParams},
    CliConfiguration,
};
use clap::Parser;
use sc_service::DatabaseSource;
use std::{
    fmt::Debug,
    fs,
    io::{self, Write},
};

/// The `purge-chain` command used to remove the whole chain.
#[derive(Debug, Clone, Parser)]
pub struct PurgeChainCmd {
    /// Skip interactive prompt by answering yes automatically.
    #[arg(short = 'y')]
    pub yes: bool,

    #[allow(missing_docs)]
    #[clap(flatten)]
    pub shared_params: SharedParams,

    #[allow(missing_docs)]
    #[clap(flatten)]
    pub database_params: DatabaseParams,
}

impl PurgeChainCmd {
    /// Run the purge command
    pub fn run(&self, database_config: DatabaseSource) -> error::Result<()> {
        let db_path = database_config
            .path()
            .and_then(|p| p.parent())
            .ok_or_else(|| {
                error::Error::Input("Cannot purge custom database implementation".into())
            })?;

        if !self.yes {
            print!("Are you sure to remove {:?}? [y/N]: ", db_path);
            io::stdout().flush().expect("failed to flush stdout");

            let mut input = String::new();
            io::stdin().read_line(&mut input)?;
            let input = input.trim();

            match input.chars().next() {
                Some('y') | Some('Y') => {}
                _ => {
                    println!("Aborted");
                    return Ok(());
                }
            }
        }

        match fs::remove_dir_all(db_path) {
            Ok(_) => {
                println!("{:?} removed.", db_path);
                Ok(())
            }
            Err(ref err) if err.kind() == io::ErrorKind::NotFound => {
                eprintln!("{:?} did not exist.", db_path);
                Ok(())
            }
            Err(err) => Result::Err(err.into()),
        }
    }
}

impl CliConfiguration for PurgeChainCmd {
    fn shared_params(&self) -> &SharedParams {
        &self.shared_params
    }

    fn database_params(&self) -> Option<&DatabaseParams> {
        Some(&self.database_params)
    }
}
