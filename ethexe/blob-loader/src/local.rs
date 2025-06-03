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

use crate::{BlobLoaderEvent, BlobLoaderService};
use anyhow::{anyhow, Result};
use ethexe_common::{CodeAndId, CodeAndIdUnchecked};
use ethexe_db::Database;
use futures::{future::BoxFuture, stream::FusedStream, FutureExt, Stream};
use gprimitives::CodeId;
use std::{
    collections::{HashMap, HashSet, VecDeque},
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
    pub async fn add_code(&self, code_and_id: CodeAndId) {
        let CodeAndIdUnchecked { code, code_id } = code_and_id.into_unchecked();
        self.inner.write().await.insert(code_id, code);
    }

    pub async fn get_code(self, code_id: CodeId) -> Result<CodeAndId> {
        let storage = self.inner.read().await;

        let Some(code) = storage.get(&code_id).cloned() else {
            return Err(anyhow!("code {code_id} not found in db"));
        };

        Ok(CodeAndId::from_unchecked(CodeAndIdUnchecked {
            code,
            code_id,
        }))
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
    future: Option<BoxFuture<'static, Result<CodeAndId>>>,
}

impl BlobLoaderService for LocalBlobLoader {
    fn into_box(self) -> Box<dyn BlobLoaderService> {
        Box::new(self)
    }

    fn pending_codes_len(&self) -> usize {
        self.codes_queue.len()
    }

    fn load_codes(&mut self, codes: HashSet<CodeId>, _attempts: Option<u8>) -> Result<()> {
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
                    Ok(code_and_id) => Poll::Ready(Some(Ok(BlobLoaderEvent::BlobLoaded(
                        code_and_id.into_unchecked(),
                    )))),
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
