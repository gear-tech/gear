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

use crate::{BlobData, BlobLoaderEvent, BlobLoaderService, Database};
use anyhow::{anyhow, Ok, Result};
use ethexe_common::CodeBlobInfo;
use futures::{
    future::BoxFuture,
    stream::{FusedStream, FuturesUnordered},
    FutureExt, Stream, StreamExt,
};
use gprimitives::CodeId;
use std::{
    collections::{HashMap, HashSet},
    sync::Arc,
    task::Poll,
};
use tokio::sync::RwLock;

#[derive(Clone, Default)]
pub struct LocalBlobStorage {
    inner: Arc<RwLock<HashMap<CodeId, Vec<u8>>>>,
}

impl LocalBlobStorage {
    pub async fn add_code(&self, code_id: CodeId, code: Vec<u8>) {
        let mut storage = self.inner.write().await;
        if storage.contains_key(&code_id) {
            return;
        }

        storage.insert(code_id, code);
    }

    pub async fn get_code(self, code_id: CodeId) -> Result<Vec<u8>> {
        let storage = self.inner.read().await;

        let Some(code) = storage.get(&code_id).cloned() else {
            return Err(anyhow!("code {code_id} not found in db"));
        };

        Ok(code)
    }
}

pub struct LocalBlobLoader<DB: Database> {
    db: DB,
    storage: LocalBlobStorage,
    futures: FuturesUnordered<BoxFuture<'static, Result<BlobData>>>,
}

impl<DB: Database> FusedStream for LocalBlobLoader<DB> {
    fn is_terminated(&self) -> bool {
        false
    }
}

impl<DB: Database> BlobLoaderService for LocalBlobLoader<DB> {
    fn into_box(self) -> Box<dyn BlobLoaderService> {
        Box::new(self)
    }

    fn pending_codes_len(&self) -> usize {
        self.futures.len()
    }

    fn load_codes(&mut self, codes: HashSet<CodeId>, _attempts: Option<u8>) -> Result<()> {
        codes.into_iter().try_for_each(|code_id| {
            let CodeBlobInfo { timestamp, .. } = self
                .db
                .code_blob_info(code_id)
                .ok_or_else(|| anyhow!("Failed to get code blob info for requested code"))?;
            let storage = self.storage.clone();
            self.futures.push(
                async move {
                    storage.get_code(code_id).await.map(|code| BlobData {
                        code_id,
                        timestamp,
                        code,
                    })
                }
                .boxed(),
            );
            Ok(())
        })
    }
}

impl<DB: Database> Stream for LocalBlobLoader<DB> {
    type Item = Result<BlobLoaderEvent>;

    fn poll_next(
        mut self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<Option<Self::Item>> {
        match self.futures.poll_next_unpin(cx) {
            Poll::Ready(Some(res)) => Poll::Ready(Some(res.map(BlobLoaderEvent::BlobLoaded))),
            _ => Poll::Pending,
        }
    }
}

impl<DB: Database> LocalBlobLoader<DB> {
    pub fn new(db: DB, storage: LocalBlobStorage) -> Self {
        Self {
            db,
            storage,
            futures: Default::default(),
        }
    }
}
