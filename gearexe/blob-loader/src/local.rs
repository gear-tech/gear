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

use crate::{BlobLoaderError, BlobLoaderEvent, BlobLoaderService, Result};
use gearexe_common::{CodeAndId, CodeAndIdUnchecked};
use futures::{
    FutureExt, Stream, StreamExt,
    future::BoxFuture,
    stream::{FusedStream, FuturesUnordered},
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
    pub fn new() -> Self {
        Self {
            inner: Arc::new(RwLock::new(HashMap::new())),
        }
    }
    pub async fn add_code(&self, code_and_id: CodeAndId) {
        let CodeAndIdUnchecked { code, code_id } = code_and_id.into_unchecked();
        self.inner.write().await.insert(code_id, code);
    }

    pub async fn get_code(self, code_id: CodeId) -> Result<CodeAndId> {
        let storage = self.inner.read().await;

        let code = storage
            .get(&code_id)
            .cloned()
            .ok_or(BlobLoaderError::LocalCodeNotFound(code_id))?;

        Ok(CodeAndId::from_unchecked(CodeAndIdUnchecked {
            code,
            code_id,
        }))
    }
}

pub struct LocalBlobLoader {
    storage: LocalBlobStorage,
    futures: FuturesUnordered<BoxFuture<'static, Result<CodeAndIdUnchecked>>>,
}

impl FusedStream for LocalBlobLoader {
    fn is_terminated(&self) -> bool {
        false
    }
}

impl BlobLoaderService for LocalBlobLoader {
    fn into_box(self) -> Box<dyn BlobLoaderService> {
        Box::new(self)
    }

    fn pending_codes_len(&self) -> usize {
        self.futures.len()
    }

    fn load_codes(&mut self, codes: HashSet<CodeId>, _attempts: Option<u8>) -> Result<()> {
        codes.into_iter().try_for_each(|code_id| {
            let storage = self.storage.clone();
            self.futures.push(
                async move {
                    storage
                        .get_code(code_id)
                        .await
                        .map(|code_and_id| code_and_id.into_unchecked())
                }
                .boxed(),
            );
            Ok(())
        })
    }
}

impl Stream for LocalBlobLoader {
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

impl LocalBlobLoader {
    pub fn new(storage: LocalBlobStorage) -> Self {
        Self {
            storage,
            futures: Default::default(),
        }
    }
}
