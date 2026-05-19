// Copyright (C) Gear Technologies Inc.
// SPDX-License-Identifier: GPL-3.0-or-later WITH Classpath-exception-2.0

//
//! Backtrace support for `gsdk`

use crate::TxStatus;
use indexmap::IndexMap;
use parking_lot::Mutex;
use std::{collections::BTreeMap, sync::Arc, time::SystemTime};
use subxt::utils::H256;

/// Transaction Status for Backtrace
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum BacktraceStatus {
    Validated,
    NoLongerInBestBlock,
    Broadcasted,
    InBestBlock {
        block_hash: H256,
        extrinsic_hash: H256,
    },
    InFinalizedBlock {
        block_hash: H256,
        extrinsic_hash: H256,
    },
    Error {
        message: String,
    },
    Dropped {
        message: String,
    },
    Invalid {
        message: String,
    },
}

impl<'s> From<&'s TxStatus> for BacktraceStatus {
    fn from(status: &'s TxStatus) -> BacktraceStatus {
        match status {
            TxStatus::Validated => BacktraceStatus::Validated,
            TxStatus::NoLongerInBestBlock => BacktraceStatus::NoLongerInBestBlock,
            TxStatus::Broadcasted => BacktraceStatus::Broadcasted,
            TxStatus::InBestBlock(b) => BacktraceStatus::InBestBlock {
                block_hash: b.block_hash(),
                extrinsic_hash: b.extrinsic_hash(),
            },
            TxStatus::InFinalizedBlock(b) => BacktraceStatus::InFinalizedBlock {
                block_hash: b.block_hash(),
                extrinsic_hash: b.extrinsic_hash(),
            },
            TxStatus::Error { message } => BacktraceStatus::Error {
                message: message.clone(),
            },
            TxStatus::Dropped { message } => BacktraceStatus::Dropped {
                message: message.clone(),
            },
            TxStatus::Invalid { message } => BacktraceStatus::Invalid {
                message: message.clone(),
            },
        }
    }
}

/// Backtrace support for transactions
#[derive(Clone, Debug, Default)]
pub struct Backtrace {
    inner: Arc<Mutex<IndexMap<H256, BTreeMap<SystemTime, BacktraceStatus>>>>,
}

impl Backtrace {
    /// Append status to transaction
    pub fn append(&self, tx: H256, status: impl Into<BacktraceStatus>) {
        let mut inner = self.inner.lock();

        if let Some(map) = inner.get_mut(&tx) {
            map.insert(SystemTime::now(), status.into());
        } else {
            let mut map: BTreeMap<SystemTime, BacktraceStatus> = Default::default();
            map.insert(SystemTime::now(), status.into());
            inner.insert(tx, map);
        };
    }

    /// Get backtrace of transaction
    pub fn get(&self, tx: H256) -> Option<BTreeMap<SystemTime, BacktraceStatus>> {
        self.inner.lock().get(&tx).cloned()
    }
}
