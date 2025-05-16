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

use crate::{BlobData, BlobLoaderEvent, BlobLoaderService};
use anyhow::{anyhow, Result};
use ethexe_common::db::{CodesStorage, OnChainStorage};
use ethexe_db::Database;
use futures::{future::BoxFuture, stream::FusedStream, FutureExt, Stream};
use gprimitives::CodeId;
use std::{
    collections::{HashMap, VecDeque},
    sync::Arc,
    task::Poll,
};
use tokio::sync::RwLock;

#[derive(Clone)]
pub struct LocalBlobStorage {
    inner: Arc<RwLock<HashMap<CodeId, Vec<u8>>>>,
    db: Database,
}

impl LocalBlobStorage {
    pub fn new(db: Database) -> Self {
        Self {
            inner: Arc::new(RwLock::new(HashMap::new())),
            db,
        }
    }
    pub async fn add_code(&self, code_id: CodeId, code: Vec<u8>) {
        let mut storage = self.inner.write().await;
        if storage.contains_key(&code_id) {
            return;
        }

        storage.insert(code_id, code);
    }

    pub async fn get_code(self, code_id: CodeId) -> Result<BlobData> {
        let storage = self.inner.read().await;

        let code = match storage.get(&code_id) {
            Some(code) => code.clone(),
            None => self.db.original_code(code_id).ok_or({
                log::error!("local storage: {storage:?}");
                anyhow!("expect code for {code_id} exists in db")
            })?,
        };

        let code_info = self
            .db
            .code_blob_info(code_id)
            .ok_or(anyhow!("expect code info for {code_id} exists in db"))?;

        Ok(BlobData {
            code_id,
            timestamp: code_info.timestamp,
            code,
        })
    }

    pub fn change_db(&mut self, db: Database) {
        self.db = db;
    }
}

impl FusedStream for LocalBlobLoader {
    fn is_terminated(&self) -> bool {
        false
    }
}

pub struct LocalBlobLoader {
    storage: LocalBlobStorage,
    codes_queue: VecDeque<CodeId>,
    future: Option<BoxFuture<'static, Result<BlobData>>>,
}

impl BlobLoaderService for LocalBlobLoader {
    fn into_box(self) -> Box<dyn BlobLoaderService> {
        Box::new(self)
    }

    fn pending_codes_len(&self) -> usize {
        self.codes_queue.len()
    }

    fn load_codes(&mut self, codes: Vec<CodeId>, _attempts: Option<u8>) -> Result<()> {
        // NOTE: This function only adds codes to the queue because of in `TestEnv` we add blob's code directly
        // to the storage using `add_code` method.

        for code_id in codes {
            self.codes_queue.push_front(code_id);
        }

        Ok(())
    }
}

impl Stream for LocalBlobLoader {
    type Item = Result<BlobLoaderEvent>;

    fn poll_next(
        mut self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<Option<Self::Item>> {
        if self.future.is_none() {
            let Some(code_id) = self.codes_queue.pop_back() else {
                return Poll::Pending;
            };
            self.future = Some(self.storage.clone().get_code(code_id).boxed());
        }

        match self.future.as_mut().unwrap().poll_unpin(cx) {
            Poll::Ready(res) => {
                self.future = None;
                match res {
                    Ok(blob_data) => {
                        self.storage.db.set_original_code(blob_data.code.as_slice());
                        Poll::Ready(Some(Ok(BlobLoaderEvent::BlobLoaded(blob_data))))
                    }
                    Err(e) => Poll::Ready(Some(Err(e))),
                }
            }
            Poll::Pending => Poll::Pending,
        }
    }
}

impl LocalBlobLoader {
    pub fn new(db: Database) -> Self {
        Self {
            storage: LocalBlobStorage::new(db.clone()),
            codes_queue: VecDeque::new(),
            future: None,
        }
    }

    pub fn from_storage(storage: LocalBlobStorage) -> Self {
        Self {
            storage,
            codes_queue: VecDeque::new(),
            future: None,
        }
    }

    #[allow(unused)]
    fn storage(&self) -> LocalBlobStorage {
        self.storage.clone()
    }
}
