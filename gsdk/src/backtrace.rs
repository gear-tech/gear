// This file is part of Gear.
//
// Copyright (C) 2021-2023 Gear Technologies Inc.
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
use sp_core::H256;
use std::{cell::RefCell, collections::BTreeMap, sync::Arc, time::SystemTime};

/// Transaction Status for Backtrace
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum BacktraceStatus {
    Future,
    Ready,
    Broadcast(Vec<String>),
    InBlock {
        block_hash: H256,
        extrinsic_hash: H256,
    },
    Retracted {
        block_hash: H256,
    },
    FinalityTimeout {
        block_hash: H256,
    },
    Finalized {
        block_hash: H256,
        extrinsic_hash: H256,
    },
    Usurped {
        extrinsic_hash: H256,
    },
    Dropped,
    Invalid,
}

impl<'s> From<&'s TxStatus> for BacktraceStatus {
    fn from(status: &'s TxStatus) -> BacktraceStatus {
        match status {
            TxStatus::Future => BacktraceStatus::Future,
            TxStatus::Ready => BacktraceStatus::Ready,
            TxStatus::Broadcast(v) => BacktraceStatus::Broadcast(v.clone()),
            TxStatus::InBlock(b) => BacktraceStatus::InBlock {
                block_hash: b.block_hash(),
                extrinsic_hash: b.extrinsic_hash(),
            },
            TxStatus::Retracted(h) => BacktraceStatus::Retracted { block_hash: *h },
            TxStatus::FinalityTimeout(h) => BacktraceStatus::FinalityTimeout { block_hash: *h },
            TxStatus::Finalized(b) => BacktraceStatus::Finalized {
                block_hash: b.block_hash(),
                extrinsic_hash: b.extrinsic_hash(),
            },
            TxStatus::Usurped(h) => BacktraceStatus::Usurped { extrinsic_hash: *h },
            TxStatus::Dropped => BacktraceStatus::Dropped,
            TxStatus::Invalid => BacktraceStatus::Invalid,
        }
    }
}

/// Backtrace support for transactions
#[derive(Clone, Debug, Default)]
pub struct Backtrace {
    inner: Arc<RefCell<IndexMap<H256, BTreeMap<SystemTime, BacktraceStatus>>>>,
}

impl Backtrace {
    /// Append status to transaction
    pub fn append(&mut self, tx: H256, status: impl Into<BacktraceStatus>) {
        if let Some(map) = self.inner.borrow_mut().get_mut(&tx) {
            map.insert(SystemTime::now(), status.into());
        } else {
            let mut map: BTreeMap<SystemTime, BacktraceStatus> = Default::default();
            map.insert(SystemTime::now(), status.into());
            self.inner.borrow_mut().insert(tx, map);
        };
    }

    /// Get backtrace of transaction
    pub fn get(&self, tx: H256) -> Option<BTreeMap<SystemTime, BacktraceStatus>> {
        self.inner.borrow().get(&tx).cloned()
    }
}
