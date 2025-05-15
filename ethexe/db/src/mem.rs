// This file is part of Gear.
//
// Copyright (C) 2024-2025 Gear Technologies Inc.
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

use crate::{CASDatabase, KVDatabase};
use dashmap::DashMap;
use gprimitives::H256;
use std::sync::Arc;

#[derive(Debug, Default, Clone)]
pub struct MemDb {
    // TODO: using Vec as key is not optimal, consider using to use another data structure.
    inner: Arc<DashMap<Vec<u8>, Vec<u8>>>,
}

impl CASDatabase for MemDb {
    fn clone_boxed(&self) -> Box<dyn CASDatabase> {
        Box::new(self.clone())
    }

    fn read(&self, hash: H256) -> Option<Vec<u8>> {
        let key = hash.as_bytes();
        self.inner.get(key).map(|v| v.value().clone())
    }

    fn contains(&self, hash: H256) -> bool {
        self.inner.contains_key(hash.as_bytes())
    }

    fn write(&self, data: &[u8]) -> H256 {
        let hash = crate::hash(data);
        self.inner.insert(hash.as_bytes().to_vec(), data.to_vec());
        hash
    }
}

impl KVDatabase for MemDb {
    fn clone_boxed(&self) -> Box<dyn KVDatabase> {
        Box::new(self.clone())
    }

    fn get(&self, key: &[u8]) -> Option<Vec<u8>> {
        self.inner.get(&key.to_vec()).map(|v| v.value().clone())
    }

    fn take(&self, key: &[u8]) -> Option<Vec<u8>> {
        self.inner.remove(&key.to_vec()).map(|(_, value)| value)
    }

    fn contains(&self, key: &[u8]) -> bool {
        self.inner.contains_key(key)
    }

    fn put(&self, key: &[u8], value: Vec<u8>) {
        self.inner.insert(key.to_vec(), value);
    }

    fn iter_prefix<'a>(
        &'a self,
        prefix: &'a [u8],
    ) -> Box<dyn Iterator<Item = (Vec<u8>, Vec<u8>)> + 'a> {
        Box::new(
            self.inner
                .iter()
                .filter(move |refs| refs.key().starts_with(prefix))
                .map(|refs| (refs.key().clone(), refs.value().clone())),
        )
    }
}

// TODO: Join tests for MemDb and RocksDb, making general tests for dyn CASDatabase.
#[cfg(test)]
mod tests {
    use super::*;
    use crate::tests;

    #[test]
    fn is_clonable() {
        tests::is_clonable(MemDb::default());
    }

    #[test]
    fn cas_read_write() {
        tests::cas_read_write(MemDb::default());
    }

    #[test]
    fn kv_read_write() {
        tests::kv_read_write(MemDb::default());
    }

    #[test]
    fn kv_iter_prefix() {
        tests::kv_iter_prefix(MemDb::default());
    }

    #[test]
    fn cas_multi_thread() {
        tests::cas_multi_thread(MemDb::default());
    }

    #[test]
    fn kv_multi_thread() {
        tests::kv_multi_thread(MemDb::default());
    }
}
