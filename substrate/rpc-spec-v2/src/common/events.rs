// This file is part of Substrate.

// Copyright (C) Parity Technologies (UK) Ltd.
// SPDX-License-Identifier: GPL-3.0-or-later WITH Classpath-exception-2.0

//! Common events for RPC-V2 spec.

use serde::{Deserialize, Serialize};

/// The storage item to query.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StorageQuery<Key> {
    /// The provided key.
    pub key: Key,
    /// The type of the storage query.
    #[serde(rename = "type")]
    pub query_type: StorageQueryType,
}

/// The storage item to query with pagination.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PaginatedStorageQuery<Key> {
    /// The provided key.
    pub key: Key,
    /// The type of the storage query.
    #[serde(rename = "type")]
    pub query_type: StorageQueryType,
    /// The pagination key from which the iteration should resume.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(default)]
    pub pagination_start_key: Option<Key>,
}

/// The type of the storage query.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum StorageQueryType {
    /// Fetch the value of the provided key.
    Value,
    /// Fetch the hash of the value of the provided key.
    Hash,
    /// Fetch the closest descendant merkle value.
    ClosestDescendantMerkleValue,
    /// Fetch the values of all descendants of they provided key.
    DescendantsValues,
    /// Fetch the hashes of the values of all descendants of they provided key.
    DescendantsHashes,
}

impl StorageQueryType {
    /// Returns `true` if the query is a descendant query.
    pub fn is_descendant_query(&self) -> bool {
        matches!(self, Self::DescendantsValues | Self::DescendantsHashes)
    }
}

/// The storage result.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StorageResult {
    /// The hex-encoded key of the result.
    pub key: String,
    /// The result of the query.
    #[serde(flatten)]
    pub result: StorageResultType,
}

/// The type of the storage query.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum StorageResultType {
    /// Fetch the value of the provided key.
    Value(String),
    /// Fetch the hash of the value of the provided key.
    Hash(String),
    /// Fetch the closest descendant merkle value.
    ClosestDescendantMerkleValue(String),
}

/// The result of a storage call.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum ArchiveStorageResult {
    /// Query generated a result.
    Ok(ArchiveStorageMethodOk),
    /// Query encountered an error.
    Err(ArchiveStorageMethodErr),
}

impl ArchiveStorageResult {
    /// Create a new `ArchiveStorageResult::Ok` result.
    pub fn ok(result: Vec<StorageResult>, discarded_items: usize) -> Self {
        Self::Ok(ArchiveStorageMethodOk {
            result,
            discarded_items,
        })
    }

    /// Create a new `ArchiveStorageResult::Err` result.
    pub fn err(error: String) -> Self {
        Self::Err(ArchiveStorageMethodErr { error })
    }
}

/// The result of a storage call.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ArchiveStorageMethodOk {
    /// Reported results.
    pub result: Vec<StorageResult>,
    /// Number of discarded items.
    pub discarded_items: usize,
}

/// The error of a storage call.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ArchiveStorageMethodErr {
    /// Reported error.
    pub error: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn storage_result() {
        // Item with Value.
        let item = StorageResult {
            key: "0x1".into(),
            result: StorageResultType::Value("res".into()),
        };
        // Encode
        let ser = serde_json::to_string(&item).unwrap();
        let exp = r#"{"key":"0x1","value":"res"}"#;
        assert_eq!(ser, exp);
        // Decode
        let dec: StorageResult = serde_json::from_str(exp).unwrap();
        assert_eq!(dec, item);

        // Item with Hash.
        let item = StorageResult {
            key: "0x1".into(),
            result: StorageResultType::Hash("res".into()),
        };
        // Encode
        let ser = serde_json::to_string(&item).unwrap();
        let exp = r#"{"key":"0x1","hash":"res"}"#;
        assert_eq!(ser, exp);
        // Decode
        let dec: StorageResult = serde_json::from_str(exp).unwrap();
        assert_eq!(dec, item);

        // Item with DescendantsValues.
        let item = StorageResult {
            key: "0x1".into(),
            result: StorageResultType::ClosestDescendantMerkleValue("res".into()),
        };
        // Encode
        let ser = serde_json::to_string(&item).unwrap();
        let exp = r#"{"key":"0x1","closestDescendantMerkleValue":"res"}"#;
        assert_eq!(ser, exp);
        // Decode
        let dec: StorageResult = serde_json::from_str(exp).unwrap();
        assert_eq!(dec, item);
    }

    #[test]
    fn storage_query() {
        // Item with Value.
        let item = StorageQuery {
            key: "0x1",
            query_type: StorageQueryType::Value,
        };
        // Encode
        let ser = serde_json::to_string(&item).unwrap();
        let exp = r#"{"key":"0x1","type":"value"}"#;
        assert_eq!(ser, exp);
        // Decode
        let dec: StorageQuery<&str> = serde_json::from_str(exp).unwrap();
        assert_eq!(dec, item);

        // Item with Hash.
        let item = StorageQuery {
            key: "0x1",
            query_type: StorageQueryType::Hash,
        };
        // Encode
        let ser = serde_json::to_string(&item).unwrap();
        let exp = r#"{"key":"0x1","type":"hash"}"#;
        assert_eq!(ser, exp);
        // Decode
        let dec: StorageQuery<&str> = serde_json::from_str(exp).unwrap();
        assert_eq!(dec, item);

        // Item with DescendantsValues.
        let item = StorageQuery {
            key: "0x1",
            query_type: StorageQueryType::DescendantsValues,
        };
        // Encode
        let ser = serde_json::to_string(&item).unwrap();
        let exp = r#"{"key":"0x1","type":"descendantsValues"}"#;
        assert_eq!(ser, exp);
        // Decode
        let dec: StorageQuery<&str> = serde_json::from_str(exp).unwrap();
        assert_eq!(dec, item);

        // Item with DescendantsHashes.
        let item = StorageQuery {
            key: "0x1",
            query_type: StorageQueryType::DescendantsHashes,
        };
        // Encode
        let ser = serde_json::to_string(&item).unwrap();
        let exp = r#"{"key":"0x1","type":"descendantsHashes"}"#;
        assert_eq!(ser, exp);
        // Decode
        let dec: StorageQuery<&str> = serde_json::from_str(exp).unwrap();
        assert_eq!(dec, item);

        // Item with Merkle.
        let item = StorageQuery {
            key: "0x1",
            query_type: StorageQueryType::ClosestDescendantMerkleValue,
        };
        // Encode
        let ser = serde_json::to_string(&item).unwrap();
        let exp = r#"{"key":"0x1","type":"closestDescendantMerkleValue"}"#;
        assert_eq!(ser, exp);
        // Decode
        let dec: StorageQuery<&str> = serde_json::from_str(exp).unwrap();
        assert_eq!(dec, item);
    }

    #[test]
    fn storage_query_paginated() {
        let item = PaginatedStorageQuery {
            key: "0x1",
            query_type: StorageQueryType::Value,
            pagination_start_key: None,
        };
        // Encode
        let ser = serde_json::to_string(&item).unwrap();
        let exp = r#"{"key":"0x1","type":"value"}"#;
        assert_eq!(ser, exp);
        // Decode
        let dec: StorageQuery<&str> = serde_json::from_str(exp).unwrap();
        assert_eq!(dec.key, item.key);
        assert_eq!(dec.query_type, item.query_type);
        let dec: PaginatedStorageQuery<&str> = serde_json::from_str(exp).unwrap();
        assert_eq!(dec, item);

        let item = PaginatedStorageQuery {
            key: "0x1",
            query_type: StorageQueryType::Value,
            pagination_start_key: Some("0x2"),
        };
        // Encode
        let ser = serde_json::to_string(&item).unwrap();
        let exp = r#"{"key":"0x1","type":"value","paginationStartKey":"0x2"}"#;
        assert_eq!(ser, exp);
        // Decode
        let dec: PaginatedStorageQuery<&str> = serde_json::from_str(exp).unwrap();
        assert_eq!(dec, item);
    }
}
