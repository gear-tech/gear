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

#[derive(Clone)]
pub enum BlobLoaderEvent {
    BlobLoaded(BlobData),
}

impl fmt::Debug for BlobLoaderEvent {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            BlobLoaderEvent::BlobLoaded(data) => data.fmt(f),
        }
    }
}

#[allow(unused)]
pub struct BlobLoaderService {
    futures: FuturesUnordered<BoxFuture<'static, Result<BlobData>>>,
    codes_loading: HashSet<CodeId>,

    blob_reader: Box<dyn BlobReader>,
    db: Database,
}

impl Stream for BlobLoaderService {
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

impl FusedStream for BlobLoaderService {
    fn is_terminated(&self) -> bool {
        false
    }
}

impl BlobLoaderService {
    pub fn new(blob_reader: Box<dyn BlobReader>, db: Database) -> Self {
        Self {
            futures: FuturesUnordered::new(),
            codes_loading: HashSet::new(),
            blob_reader,
            db,
        }
    }

    pub fn load_codes(&mut self, codes: Vec<CodeId>, attempts: Option<u8>) -> Result<()> {
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
