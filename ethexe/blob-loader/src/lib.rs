use crate::blobs::{BlobData, BlobReader};
use anyhow::{Error, Result};
use ethexe_db::Database;
use gprimitives::{CodeId, H256};
use mapped_futures::mapped_futures::MappedFutures;
use std::{collections::HashSet, pin::Pin};

use utils::*;

pub mod blobs;
pub mod utils;

type CodeLoadFuture = Pin<Box<dyn Future<Output = Result<BlobData, Error>> + Send>>;

pub enum BlobLoaderEvent {
    LoadedCodes(Vec<CodeId>),
}

#[allow(unused)]
pub struct BlobLoaderService {
    blob_reader: Box<dyn BlobReader>,
    codes_futures: MappedFutures<CodeId, CodeLoadFuture>,
    db: Database,
}

impl BlobLoaderService {
    pub fn new(blob_reader: Box<dyn BlobReader>, db: Database) -> Self {
        Self {
            blob_reader,
            codes_futures: MappedFutures::new(),
            db,
        }
    }

    pub fn load_code(
        &mut self,
        _code_id: CodeId,
        _timestamp: u64,
        _tx_hash: H256,
        _attempts: Option<u8>,
    ) {
        todo!();
    }

    pub async fn load_codes_now(&self, _codes: HashSet<CodeId>) -> Result<()> {
        todo!();
    }

    pub fn receive_load_request(&mut self, _codes: HashSet<CodeId>) {
        todo!();
    }

    pub fn poll_next(&mut self) -> Result<BlobData> {
        todo!()
    }
}
