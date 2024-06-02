// This file is part of Gear.
//
// Copyright (C) 2024 Gear Technologies Inc.
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

use crate::{CASDatabase, Database, State};
use anyhow::Result;
use gear_core::{code::InstrumentedCode, ids};
use gprimitives::{CodeId, H256};
use rocksdb::{Options, DB};
use std::{path::PathBuf, sync::Arc};

/// Database for storing states and codes in memory.
#[derive(Debug, Clone)]
pub struct RocksDatabase(Arc<DB>);

impl RocksDatabase {
    fn hash(data: &[u8]) -> H256 {
        H256::from(ids::hash(data))
    }

    /// Open database at specified
    pub fn open(path: PathBuf) -> Result<Self> {
        let db = DB::open(&configure_rocksdb(), path)?;
        Ok(Self(Arc::new(db)))
    }
}

impl Database for RocksDatabase {
    fn clone_boxed(&self) -> Box<dyn Database> {
        Box::new(self.clone())
    }

    fn read_code(&self, _code_id: CodeId) -> Option<Vec<u8>> {
        unimplemented!()
    }

    fn write_code(&self, _code_id: CodeId, _code: &[u8]) {
        unimplemented!()
    }

    fn read_instrumented_code(&self, _code_id: CodeId) -> Option<InstrumentedCode> {
        unimplemented!()
    }

    fn write_instrumented_code(&self, _code_id: CodeId, _code: &InstrumentedCode) {
        unimplemented!()
    }

    fn read_state(&self, _hash: H256) -> Option<State> {
        unimplemented!()
    }

    fn write_state(&self, _state: &State) {
        unimplemented!()
    }
}

impl CASDatabase for RocksDatabase {
    fn clone_boxed(&self) -> Box<dyn CASDatabase> {
        Box::new(self.clone())
    }

    fn read(&self, hash: &H256) -> Option<Vec<u8>> {
        self.0
            .get(hash.as_bytes())
            .expect("Failed to read data, database is not in valid state")
    }

    fn write(&self, data: &[u8]) -> H256 {
        let hash = Self::hash(data);
        self.0
            .put(hash.as_bytes(), data)
            .expect("Failed to write data, database is not in valid state");
        hash
    }
}

// TODO: Tune RocksDB configuration.
fn configure_rocksdb() -> Options {
    let mut opts = Options::default();
    opts.create_if_missing(true);
    opts.increase_parallelism(8);
    opts.set_max_background_jobs(4);
    opts.set_write_buffer_size(64 * 1024 * 1024);
    opts.set_max_write_buffer_number(3);
    opts.set_use_fsync(false);

    opts
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::thread::{self};
    use tempfile::TempDir;

    fn with_database<F>(f: F)
    where
        F: FnOnce(RocksDatabase),
    {
        let temp_dir = scopeguard::guard(
            TempDir::new().expect("Failed to create a temporary directory"),
            |temp_dir| {
                temp_dir
                    .close()
                    .expect("Failed to close the temporary directory");
            },
        );

        f(RocksDatabase::open(temp_dir.path().to_path_buf()).expect("Failed to open database"));
    }

    #[test]
    fn is_clonable() {
        with_database(|db| {
            let _ = db.clone();
        });
    }

    #[test]
    fn read_write() {
        with_database(|db| {
            let data = b"Hello, world!";
            let hash = db.write(data);

            assert_eq!(db.read(&hash), Some(data.to_vec()));
        });
    }

    #[test]
    fn multi_thread() {
        let amount = 10;

        let to_big_vec = |x: u32| -> Vec<u8> {
            let bytes = x.to_le_bytes();
            bytes
                .iter()
                .cycle()
                .take(1024 * 1024)
                .copied()
                .collect::<Vec<_>>()
        };

        with_database(|db| {
            let db_clone = CASDatabase::clone_boxed(&db);
            let handler1 = thread::spawn(move || {
                for x in 0u32..amount {
                    db_clone.write(to_big_vec(x).as_slice());
                }
            });

            let db_clone = CASDatabase::clone_boxed(&db);
            let handler2 = thread::spawn(move || {
                for x in amount..amount * 2 {
                    db_clone.write(to_big_vec(x).as_slice());
                }
            });

            handler1.join().unwrap();
            handler2.join().unwrap();

            for x in 0u32..amount * 2 {
                let data = to_big_vec(x);
                let hash = db.read(&RocksDatabase::hash(data.as_slice()));
                assert_eq!(hash, Some(data));
            }
        })
    }
}
