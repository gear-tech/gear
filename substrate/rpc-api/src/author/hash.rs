// This file is part of Substrate.

// Copyright (C) Parity Technologies (UK) Ltd.
// SPDX-License-Identifier: GPL-3.0-or-later WITH Classpath-exception-2.0

//! Extrinsic helpers for author RPC module.

use serde::{Deserialize, Serialize};
use sp_core::Bytes;

/// RPC Extrinsic or hash
///
/// Allows to refer to extrinsic either by its raw representation or its hash.
#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ExtrinsicOrHash<Hash> {
    /// The hash of the extrinsic.
    Hash(Hash),
    /// Raw extrinsic bytes.
    Extrinsic(Bytes),
}
