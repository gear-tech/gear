// This file is part of Substrate.

// Copyright (C) Parity Technologies (UK) Ltd.
// SPDX-License-Identifier: GPL-3.0-or-later WITH Classpath-exception-2.0

use super::*;
use crate::testing::{allow_unsafe, deny_unsafe};
use assert_matches::assert_matches;
use sp_core::{offchain::storage::InMemOffchainStorage, Bytes};

#[test]
fn local_storage_should_work() {
    let storage = InMemOffchainStorage::default();
    let offchain = Offchain::new(storage);
    let key = Bytes(b"offchain_storage".to_vec());
    let value = Bytes(b"offchain_value".to_vec());

    let ext = allow_unsafe();

    assert_matches!(
        offchain.set_local_storage(&ext, StorageKind::PERSISTENT, key.clone(), value.clone()),
        Ok(())
    );
    assert_matches!(
        offchain.get_local_storage(&ext, StorageKind::PERSISTENT, key),
        Ok(Some(ref v)) if *v == value
    );
}

#[test]
fn offchain_calls_considered_unsafe() {
    let storage = InMemOffchainStorage::default();
    let offchain = Offchain::new(storage);
    let key = Bytes(b"offchain_storage".to_vec());
    let value = Bytes(b"offchain_value".to_vec());

    let ext = deny_unsafe();

    assert_matches!(
        offchain.set_local_storage(&ext, StorageKind::PERSISTENT, key.clone(), value.clone()),
        Err(Error::UnsafeRpcCalled(e)) => {
            assert_eq!(e.to_string(), "RPC call is unsafe to be called externally")
        }
    );
    assert_matches!(
        offchain.get_local_storage(&ext, StorageKind::PERSISTENT, key),
        Err(Error::UnsafeRpcCalled(e)) => {
            assert_eq!(e.to_string(), "RPC call is unsafe to be called externally")
        }
    );
}
