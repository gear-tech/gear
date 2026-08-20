// Copyright (C) Parity Technologies (UK) Ltd.
// SPDX-License-Identifier: GPL-3.0-or-later WITH Classpath-exception-2.0

use clap::Args;
use sc_service::config::TransactionPoolOptions;

/// Parameters used to create the pool configuration.
#[derive(Debug, Clone, Args)]
pub struct TransactionPoolParams {
    /// Maximum number of transactions in the transaction pool.
    #[arg(long, value_name = "COUNT", default_value_t = 8192)]
    pub pool_limit: usize,

    /// Maximum number of kilobytes of all transactions stored in the pool.
    #[arg(long, value_name = "COUNT", default_value_t = 20480)]
    pub pool_kbytes: usize,

    /// How long a transaction is banned for.
    ///
    /// If it is considered invalid. Defaults to 1800s.
    #[arg(long, value_name = "SECONDS")]
    pub tx_ban_seconds: Option<u64>,
}

impl TransactionPoolParams {
    /// Fill the given `PoolConfiguration` by looking at the cli parameters.
    pub fn transaction_pool(&self, is_dev: bool) -> TransactionPoolOptions {
        let mut opts = TransactionPoolOptions::default();

        // ready queue
        opts.ready.count = self.pool_limit;
        opts.ready.total_bytes = self.pool_kbytes * 1024;

        // future queue
        let factor = 10;
        opts.future.count = self.pool_limit / factor;
        opts.future.total_bytes = self.pool_kbytes * 1024 / factor;

        opts.ban_time = if let Some(ban_seconds) = self.tx_ban_seconds {
            std::time::Duration::from_secs(ban_seconds)
        } else if is_dev {
            std::time::Duration::from_secs(0)
        } else {
            std::time::Duration::from_secs(30 * 60)
        };

        opts
    }
}
