// This file is part of Gear.
//
// Copyright (C) 2021-2025 Gear Technologies Inc.
// SPDX-License-Identifier: GPL-3.0-or-later WITH Classpath-exception-2.0
//
// This program is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.
//
// This program is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE. See the
// GNU General Public License for more details.
//
// You should have received a copy of the GNU General Public License
// along with this program. If not, see <https://www.gnu.org/licenses/>.
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
    Broadcasted {
        num_peers: u32,
    },
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
            TxStatus::Broadcasted { num_peers } => BacktraceStatus::Broadcasted {
                num_peers: *num_peers,
            },
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
