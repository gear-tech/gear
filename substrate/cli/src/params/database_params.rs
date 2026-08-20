// Copyright (C) Parity Technologies (UK) Ltd.
// SPDX-License-Identifier: GPL-3.0-or-later WITH Classpath-exception-2.0

use crate::arg_enums::Database;
use clap::Args;

/// Parameters for database
#[derive(Debug, Clone, PartialEq, Args)]
pub struct DatabaseParams {
    /// Select database backend to use.
    #[arg(long, alias = "db", value_name = "DB", ignore_case = true, value_enum)]
    pub database: Option<Database>,

    /// Limit the memory the database cache can use.
    #[arg(long = "db-cache", value_name = "MiB")]
    pub database_cache_size: Option<usize>,
}

impl DatabaseParams {
    /// Database backend
    pub fn database(&self) -> Option<Database> {
        self.database
    }

    /// Limit the memory the database cache can use.
    pub fn database_cache_size(&self) -> Option<usize> {
        self.database_cache_size
    }
}
