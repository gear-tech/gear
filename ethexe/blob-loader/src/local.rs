// This file is part of Gear.
//
// Copyright (C) 2025 Gear Technologies Inc.
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

use crate::{blobs::BlobData, BlobLoaderEvent, BlobLoaderService};
use anyhow::{anyhow, Result};
use ethexe_common::db::{CodeInfo, CodesStorage, OnChainStorage};
use ethexe_db::Database;
use futures::{future::BoxFuture, FutureExt, Stream};
use gprimitives::{CodeId, H256};
use std::{
    collections::{HashMap, VecDeque},
    sync::Arc,
    task::Poll,
};
use tokio::sync::RwLock;

struct StorageData {
    pub codes_map: HashMap<CodeId, Vec<u8>>,
    pub codes_queue: VecDeque<CodeId>,
}

#[derive(Clone)]
pub struct LocalBlobStorage {
    inner: Arc<RwLock<StorageData>>,
    db: Database,
}

impl LocalBlobStorage {
    pub fn new(db: Database) -> Self {
        Self {
            inner: Arc::new(RwLock::new(StorageData {
                codes_map: HashMap::new(),
                codes_queue: VecDeque::new(),
            })),
            db,
        }
    }

    pub async fn add_code(&mut self, code_id: CodeId, code: Vec<u8>) {
        let mut storage_data = self.inner.write().await;
        if storage_data.codes_map.contains_key(&code_id) {
            return;
        }

        storage_data.codes_map.insert(code_id, code);
        storage_data.codes_queue.push_front(code_id);
    }

    pub async fn next_code(&mut self, attempts: Option<u8>) -> Result<Option<BlobData>> {
        let mut storage_data = self.inner.write().await;
        let code_id = match storage_data.codes_queue.pop_back() {
            Some(code_id) => code_id,
            None => Ok(None),
        };

        let code_info = self
            .db
            .code_blob_info(code_id)
            .ok_or(anyhow!("not found in db code info for {code_id}"))?;
        let code = storage_data
            .codes_map
            .remove(&code_id)
            .ok_or(anyhow!("nof found code in local storage for {code_id}"))?;

        return Ok(Some(BlobData {
            code_id,
            timestamp: code_info.timestamp,
            code,
        }));
    }
}

pub struct LocalBlobLoader {
    storage: LocalBlobStorage,
    future: BoxFuture<'static, Result<Option<BlobData>>>,
}

impl BlobLoaderService for LocalBlobLoader {
    fn load_codes(&mut self, codes: Vec<CodeId>, attempts: Option<u8>) -> Result<()> {
        // In local implementation we just add codes to local storage directly
        Ok(())
    }
}

impl Stream for LocalBlobLoader {
    type Item = Result<BlobLoaderEvent>;

    fn poll_next(
        mut self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<Option<Self::Item>> {
        match self.future.poll_unpin(cx) {
            Poll::Ready(result) => match result {
                Ok(Some(blob_data)) => {
                    self.future = self.storage.next_code(None);
                    return Poll::Ready(Some(Ok(BlobLoaderEvent::BlobLoaded(blob_data))));
                }
                Ok(None) => Poll::Pending,
                Err(err) => Poll::Ready(Some(Err(err))),
            },
            _ => Poll::Pending,
        }
    }
}

impl LocalBlobLoader {
    fn new(storage: LocalBlobStorage) -> Self {
        Self {
            storage,
            future: storage.next_code(None),
        }
    }
}
