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

use crate::blobs::{BlobData, BlobReader};
use anyhow::{anyhow, Result};
use ethexe_common::db::{CodesStorage, OnChainStorage};
use ethexe_db::Database;
use futures::{
    future::BoxFuture,
    stream::{FusedStream, FuturesUnordered},
    FutureExt, Stream, StreamExt,
};
use gprimitives::CodeId;
use std::{collections::HashSet, fmt, pin::Pin, task::Poll};

use utils::*;

pub mod blobs;
pub mod utils;
pub mod local;

#[derive(Clone)]
pub enum BlobLoaderEvent {
    BlobLoaded(BlobData),
}

pub trait BlobLoaderService{
    fn load_codes(&mut self, codes: Vec<CodeId>, attempts: Option<u8>) -> Result<()>;
}

impl fmt::Debug for BlobLoaderEvent {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            BlobLoaderEvent::BlobLoaded(data) => data.fmt(f),
        }
    }
}

pub struct BlobLoader {
    futures: FuturesUnordered<BoxFuture<'static, Result<BlobData>>>,
    codes_loading: HashSet<CodeId>,

    blob_reader: Box<dyn BlobReader>,
    db: Database,
}

impl Stream for BlobLoader {
    type Item = Result<BlobLoaderEvent>;

    fn poll_next(
        mut self: Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<Option<Self::Item>> {
        let future = self.futures.poll_next_unpin(cx);
        match future {
            Poll::Ready(Some(result)) => match result {
                Ok(blob_data) => {
                    let code_id = &blob_data.code_id;
                    self.codes_loading.remove(code_id);
                    self.db.set_original_code(blob_data.code.as_slice());
                    Poll::Ready(Some(Ok(BlobLoaderEvent::BlobLoaded(blob_data))))
                }
                Err(e) => Poll::Ready(Some(Err(e))),
            },
            _ => Poll::Pending,
        }
    }
}

impl FusedStream for BlobLoader {
    fn is_terminated(&self) -> bool {
        false
    }
}

impl BlobLoader{
    pub fn new(blob_reader: Box<dyn BlobReader>, db: Database) -> Self {
        Self {
            futures: FuturesUnordered::new(),
            codes_loading: HashSet::new(),
            blob_reader,
            db,
        }
    }
}

impl BlobLoaderService for BlobLoader{
    fn load_codes(&mut self, codes: Vec<CodeId>, attempts: Option<u8>) -> Result<()> {
        log::info!("Request load codes: {codes:?}");
        for code_id in codes {
            if self.codes_loading.contains(&code_id) || self.db.original_code_exists(code_id) {
                continue;
            }

            let code_info = self
                .db
                .code_blob_info(code_id)
                .ok_or_else(|| anyhow!("Not found {code_id} in db"))?;

            self.codes_loading.insert(code_id);
            self.futures.push(
                crate::read_code_from_tx_hash(
                    self.blob_reader.clone(),
                    code_id,
                    code_info.timestamp,
                    code_info.tx_hash,
                    attempts,
                )
                .boxed(),
            );
        }

        Ok(())
    }
}
