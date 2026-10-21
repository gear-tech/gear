// This file is part of Substrate.

// Copyright (C) Parity Technologies (UK) Ltd.
// SPDX-License-Identifier: GPL-3.0-or-later WITH Classpath-exception-2.0

//! Storage queries for the RPC-V2 spec.

use std::{cell::Cell, marker::PhantomData, sync::Arc};

use sc_client_api::{Backend, ChildInfo, StorageKey, StorageProvider};
use sp_runtime::traits::Block as BlockT;

use super::events::{StorageResult, StorageResultType};
use crate::hex_string;

/// Call into the storage of blocks.
pub struct Storage<Client, Block, BE> {
    /// Substrate client.
    client: Arc<Client>,
    remaining_response_bytes: Cell<Option<usize>>,
    _phandom: PhantomData<(BE, Block)>,
}

impl<Client, Block, BE> Storage<Client, Block, BE> {
    /// Constructs a new [`Storage`].
    pub fn new(client: Arc<Client>) -> Self {
        Self {
            client,
            remaining_response_bytes: Cell::new(None),
            _phandom: PhantomData,
        }
    }

    /// Limit the next batch before allocating hex-encoded results.
    pub(crate) fn limit_response_size(&mut self, bytes: usize) {
        self.remaining_response_bytes.set(Some(bytes));
    }

    fn charge_result(&self, key: &[u8], value: &[u8]) -> Result<(), String> {
        if let Some(remaining) = self.remaining_response_bytes.get() {
            let bytes = key
                .len()
                .saturating_add(value.len())
                .saturating_mul(2)
                .saturating_add(4 + std::mem::size_of::<StorageResult>());
            let remaining = remaining
                .checked_sub(bytes)
                .ok_or_else(|| "Storage response exceeds the byte limit".to_string())?;
            self.remaining_response_bytes.set(Some(remaining));
        }
        Ok(())
    }
}

/// Query to iterate over storage.
pub struct QueryIter {
    /// The key from which the iteration was started.
    pub query_key: StorageKey,
    /// The key after which pagination should resume.
    pub pagination_start_key: Option<StorageKey>,
    /// The type of the query (either value or hash).
    pub ty: IterQueryType,
}

/// The query type of an iteration.
pub enum IterQueryType {
    /// Iterating over (key, value) pairs.
    Value,
    /// Iterating over (key, hash) pairs.
    Hash,
}

/// The result of making a query call.
pub type QueryResult = Result<Option<StorageResult>, String>;

/// The result of iterating over keys.
pub type QueryIterResult = Result<(Vec<StorageResult>, Option<QueryIter>), String>;

impl<Client, Block, BE> Storage<Client, Block, BE>
where
    Block: BlockT + 'static,
    BE: Backend<Block> + 'static,
    Client: StorageProvider<Block, BE> + 'static,
{
    /// Fetch the value from storage.
    pub fn query_value(
        &self,
        hash: Block::Hash,
        key: &StorageKey,
        child_key: Option<&ChildInfo>,
    ) -> QueryResult {
        let result = if let Some(child_key) = child_key {
            self.client.child_storage(hash, child_key, key)
        } else {
            self.client.storage(hash, key)
        };

        result
            .map_err(|error| error.to_string())?
            .map(|storage_data| {
                self.charge_result(&key.0, &storage_data.0)?;
                Ok(StorageResult {
                    key: hex_string(&key.0),
                    result: StorageResultType::Value(hex_string(&storage_data.0)),
                })
            })
            .transpose()
    }

    /// Fetch the hash of a value from storage.
    pub fn query_hash(
        &self,
        hash: Block::Hash,
        key: &StorageKey,
        child_key: Option<&ChildInfo>,
    ) -> QueryResult {
        let result = if let Some(child_key) = child_key {
            self.client.child_storage_hash(hash, child_key, key)
        } else {
            self.client.storage_hash(hash, key)
        };

        result
            .map_err(|error| error.to_string())?
            .map(|storage_data| {
                self.charge_result(&key.0, storage_data.as_ref())?;
                Ok(StorageResult {
                    key: hex_string(&key.0),
                    result: StorageResultType::Hash(hex_string(&storage_data.as_ref())),
                })
            })
            .transpose()
    }

    /// Fetch the closest merkle value.
    pub fn query_merkle_value(
        &self,
        hash: Block::Hash,
        key: &StorageKey,
        child_key: Option<&ChildInfo>,
    ) -> QueryResult {
        let result = if let Some(child_key) = child_key {
            self.client.child_closest_merkle_value(hash, child_key, key)
        } else {
            self.client.closest_merkle_value(hash, key)
        };

        result
            .map_err(|error| error.to_string())?
            .map(|storage_data| {
                let value = match &storage_data {
                    sc_client_api::MerkleValue::Node(data) => data.as_slice(),
                    sc_client_api::MerkleValue::Hash(hash) => hash.as_ref(),
                };
                self.charge_result(&key.0, value)?;
                Ok(StorageResult {
                    key: hex_string(&key.0),
                    result: StorageResultType::ClosestDescendantMerkleValue(hex_string(&value)),
                })
            })
            .transpose()
    }

    /// Iterate over at most the provided number of keys.
    ///
    /// Returns the storage result with a potential next key to resume iteration.
    pub fn query_iter_pagination(
        &self,
        query: QueryIter,
        hash: Block::Hash,
        child_key: Option<&ChildInfo>,
        count: usize,
    ) -> QueryIterResult {
        let QueryIter {
            ty,
            query_key,
            pagination_start_key,
        } = query;

        let mut keys_iter = if let Some(child_key) = child_key {
            self.client.child_storage_keys(
                hash,
                child_key.to_owned(),
                Some(&query_key),
                pagination_start_key.as_ref(),
            )
        } else {
            self.client
                .storage_keys(hash, Some(&query_key), pagination_start_key.as_ref())
        }
        .map_err(|err| err.to_string())?;

        let mut ret = Vec::with_capacity(count);
        let mut next_pagination_key = None;
        for _ in 0..count {
            let Some(key) = keys_iter.next() else { break };

            next_pagination_key = Some(key.clone());

            let result = match ty {
                IterQueryType::Value => self.query_value(hash, &key, child_key),
                IterQueryType::Hash => self.query_hash(hash, &key, child_key),
            }?;

            if let Some(value) = result {
                ret.push(value);
            }
        }

        // Save the next key if any to continue the iteration.
        let maybe_next_query = keys_iter.next().map(|_| QueryIter {
            ty,
            query_key,
            pagination_start_key: next_pagination_key,
        });
        Ok((ret, maybe_next_query))
    }
}
