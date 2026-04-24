// This file is part of Gear.
//
// Copyright (C) 2026 Gear Technologies Inc.
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

//! Persistent store for the Malachite channel app, backed by `ethexe-db`.
//!
//! Flat KV layout with 1-byte table prefixes so keys never collide
//! with the 32-byte content-addressed keys that `ethexe-db`'s CAS
//! writes to the same underlying RocksDB instance.

use std::path::Path;
use std::sync::Arc;

use bytes::Bytes;
use ethexe_db::{KVDatabase, RocksDatabase};
use thiserror::Error;

use malachitebft_app_channel::app::types::ProposedValue;
use malachitebft_app_channel::app::types::codec::Codec;
use malachitebft_app_channel::app::types::core::{CommitCertificate, Round};
use malachitebft_core_types::Value as _ValueTrait;

use crate::codec::JsonCodec;
use crate::context::{EthexeContext, Height, Value, ValueId};
use crate::streaming::ProposalParts;

// ---- key schema ---------------------------------------------------------

const P_DECIDED_VALUE: u8 = 0x01;
const P_CERTIFICATE: u8 = 0x02;
const P_UNDECIDED: u8 = 0x03;
const P_PENDING_PARTS: u8 = 0x04;

fn encode_round(round: Round) -> [u8; 8] {
    ((round.as_i64() as u64) ^ 0x8000_0000_0000_0000_u64).to_be_bytes()
}

fn key_decided_value(h: Height) -> [u8; 9] {
    let mut k = [0u8; 9];
    k[0] = P_DECIDED_VALUE;
    k[1..9].copy_from_slice(&h.as_u64().to_be_bytes());
    k
}

fn key_certificate(h: Height) -> [u8; 9] {
    let mut k = [0u8; 9];
    k[0] = P_CERTIFICATE;
    k[1..9].copy_from_slice(&h.as_u64().to_be_bytes());
    k
}

fn key_undecided(h: Height, r: Round, v: ValueId) -> [u8; 49] {
    let mut k = [0u8; 49];
    k[0] = P_UNDECIDED;
    k[1..9].copy_from_slice(&h.as_u64().to_be_bytes());
    k[9..17].copy_from_slice(&encode_round(r));
    k[17..49].copy_from_slice(v.0.as_fixed_bytes());
    k
}

fn key_pending(h: Height, r: Round, v: ValueId) -> [u8; 49] {
    let mut k = [0u8; 49];
    k[0] = P_PENDING_PARTS;
    k[1..9].copy_from_slice(&h.as_u64().to_be_bytes());
    k[9..17].copy_from_slice(&encode_round(r));
    k[17..49].copy_from_slice(v.0.as_fixed_bytes());
    k
}

fn prefix_undecided_hr(h: Height, r: Round) -> [u8; 17] {
    let mut p = [0u8; 17];
    p[0] = P_UNDECIDED;
    p[1..9].copy_from_slice(&h.as_u64().to_be_bytes());
    p[9..17].copy_from_slice(&encode_round(r));
    p
}

fn prefix_pending_hr(h: Height, r: Round) -> [u8; 17] {
    let mut p = [0u8; 17];
    p[0] = P_PENDING_PARTS;
    p[1..9].copy_from_slice(&h.as_u64().to_be_bytes());
    p[9..17].copy_from_slice(&encode_round(r));
    p
}

fn decode_height_from_key(k: &[u8]) -> Option<Height> {
    if k.len() < 9 {
        return None;
    }
    let bytes = k[1..9].try_into().ok()?;
    Some(Height::new(u64::from_be_bytes(bytes)))
}

// ---- codec helpers ------------------------------------------------------
//
// JSON instead of protobuf — the whole codec story flipped to serde
// when we swapped in our own `Context`.

type CodecError = serde_json::Error;

fn decode_certificate(bytes: &[u8]) -> Result<CommitCertificate<EthexeContext>, CodecError> {
    <JsonCodec as Codec<CommitCertificate<EthexeContext>>>::decode(
        &JsonCodec,
        Bytes::copy_from_slice(bytes),
    )
}

fn encode_certificate(cert: &CommitCertificate<EthexeContext>) -> Result<Vec<u8>, CodecError> {
    <JsonCodec as Codec<CommitCertificate<EthexeContext>>>::encode(&JsonCodec, cert)
        .map(|b| b.to_vec())
}

// ---- public types -------------------------------------------------------

#[derive(Clone, Debug)]
pub struct DecidedValue {
    pub value: Value,
    pub certificate: CommitCertificate<EthexeContext>,
}

#[derive(Debug, Error)]
pub enum StoreError {
    #[error("Backend error: {0}")]
    Backend(String),

    #[error("Failed to join on task: {0}")]
    TaskJoin(#[from] tokio::task::JoinError),

    #[error("Failed to serialize/deserialize JSON: {0}")]
    Serialization(#[from] serde_json::Error),
}

// ---- Inner blocking DB --------------------------------------------------

struct Db {
    db: RocksDatabase,
}

impl Db {
    fn new(path: impl AsRef<Path>) -> Result<Self, StoreError> {
        let db = RocksDatabase::open(path.as_ref().to_path_buf())
            .map_err(|e| StoreError::Backend(e.to_string()))?;
        Ok(Self { db })
    }

    fn insert_decided_value(&self, dv: DecidedValue) -> Result<(), StoreError> {
        let height = dv.certificate.height;
        let value_bytes = <JsonCodec as Codec<Value>>::encode(&JsonCodec, &dv.value)?.to_vec();
        let cert_bytes = encode_certificate(&dv.certificate)?;
        self.db.put(&key_decided_value(height), value_bytes);
        self.db.put(&key_certificate(height), cert_bytes);
        Ok(())
    }

    fn get_decided_value(&self, height: Height) -> Result<Option<DecidedValue>, StoreError> {
        let value = self
            .db
            .get(&key_decided_value(height))
            .and_then(|b| {
                <JsonCodec as Codec<Value>>::decode(&JsonCodec, Bytes::from(b)).ok()
            });
        let cert = self
            .db
            .get(&key_certificate(height))
            .and_then(|b| decode_certificate(&b).ok());
        Ok(value
            .zip(cert)
            .map(|(value, certificate)| DecidedValue { value, certificate }))
    }

    fn insert_undecided_proposal(
        &self,
        p: ProposedValue<EthexeContext>,
    ) -> Result<(), StoreError> {
        let key = key_undecided(p.height, p.round, p.value.id());
        let bytes = JsonCodec.encode(&p)?;
        self.db.put(&key, bytes.to_vec());
        Ok(())
    }

    fn get_undecided_proposal(
        &self,
        height: Height,
        round: Round,
        value_id: ValueId,
    ) -> Result<Option<ProposedValue<EthexeContext>>, StoreError> {
        let key = key_undecided(height, round, value_id);
        if let Some(bytes) = self.db.get(&key) {
            let p: ProposedValue<EthexeContext> = JsonCodec.decode(Bytes::from(bytes))?;
            Ok(Some(p))
        } else {
            Ok(None)
        }
    }

    fn get_undecided_proposals(
        &self,
        height: Height,
        round: Round,
    ) -> Result<Vec<ProposedValue<EthexeContext>>, StoreError> {
        let prefix = prefix_undecided_hr(height, round);
        let mut out = Vec::new();
        for (_, v) in self.db.iter_prefix(&prefix) {
            let p: ProposedValue<EthexeContext> = JsonCodec.decode(Bytes::from(v))?;
            out.push(p);
        }
        Ok(out)
    }

    fn get_undecided_proposal_by_value_id(
        &self,
        value_id: ValueId,
    ) -> Result<Option<ProposedValue<EthexeContext>>, StoreError> {
        for (_, v) in self.db.iter_prefix(&[P_UNDECIDED]) {
            let p: ProposedValue<EthexeContext> = JsonCodec.decode(Bytes::from(v))?;
            if p.value.id() == value_id {
                return Ok(Some(p));
            }
        }
        Ok(None)
    }

    fn insert_pending_proposal_parts(&self, parts: ProposalParts) -> Result<(), StoreError> {
        let vid = generate_value_id_from_parts(&parts);
        let key = key_pending(parts.height, parts.round, vid);
        let bytes = serde_json::to_vec(&parts)?;
        self.db.put(&key, bytes);
        Ok(())
    }

    fn get_pending_proposal_parts(
        &self,
        height: Height,
        round: Round,
    ) -> Result<Vec<ProposalParts>, StoreError> {
        let prefix = prefix_pending_hr(height, round);
        let mut out = Vec::new();
        for (_, v) in self.db.iter_prefix(&prefix) {
            let p: ProposalParts = serde_json::from_slice(&v)?;
            out.push(p);
        }
        Ok(out)
    }

    fn remove_pending_proposal_parts(&self, parts: ProposalParts) -> Result<(), StoreError> {
        let vid = generate_value_id_from_parts(&parts);
        let key = key_pending(parts.height, parts.round, vid);
        // SAFETY: remove-on-commit is exactly what we want here.
        unsafe {
            self.db.take(&key);
        }
        Ok(())
    }

    fn prune(&self, current_height: Height, retain_height: Height) -> Result<(), StoreError> {
        let mut to_delete: Vec<Vec<u8>> = Vec::new();

        for p in [&[P_DECIDED_VALUE][..], &[P_CERTIFICATE][..]] {
            for (k, _) in self.db.iter_prefix(p) {
                if let Some(h) = decode_height_from_key(&k)
                    && h < retain_height {
                        to_delete.push(k);
                    }
            }
        }
        for p in [&[P_UNDECIDED][..], &[P_PENDING_PARTS][..]] {
            for (k, _) in self.db.iter_prefix(p) {
                if let Some(h) = decode_height_from_key(&k)
                    && h <= current_height {
                        to_delete.push(k);
                    }
            }
        }
        for k in to_delete {
            // SAFETY: bulk removal of stale consensus state.
            unsafe {
                self.db.take(&k);
            }
        }
        Ok(())
    }

    fn min_decided_value_height(&self) -> Option<Height> {
        let mut min: Option<Height> = None;
        for (k, _) in self.db.iter_prefix(&[P_DECIDED_VALUE]) {
            if let Some(h) = decode_height_from_key(&k) {
                min = Some(min.map_or(h, |m| m.min(h)));
            }
        }
        min
    }

    fn max_decided_value_height(&self) -> Option<Height> {
        let mut max: Option<Height> = None;
        for (k, _) in self.db.iter_prefix(&[P_DECIDED_VALUE]) {
            if let Some(h) = decode_height_from_key(&k) {
                max = Some(max.map_or(h, |m| m.max(h)));
            }
        }
        max
    }
}

/// Derive a stable `ValueId` from a proposal's assembled parts — matches
/// the upstream `examples/channel` derivation so streaming semantics
/// are unchanged.
pub fn generate_value_id_from_parts(parts: &ProposalParts) -> ValueId {
    use parity_scale_codec::Encode;
    use sha3::{Digest, Keccak256};

    let mut hasher = Keccak256::new();
    hasher.update(parts.height.as_u64().to_be_bytes());
    hasher.update(parts.round.as_i64().to_be_bytes());
    hasher.update(parts.proposer.into_inner());
    for part in &parts.parts {
        if let Some(data) = part.as_data() {
            hasher.update(data.block.encode());
        }
    }
    let hash = hasher.finalize();
    ValueId(gprimitives::H256::from_slice(&hash))
}

// ---- async facade -------------------------------------------------------

#[derive(Clone)]
pub struct Store {
    db: Arc<Db>,
}

impl Store {
    pub async fn open(path: impl AsRef<Path>) -> Result<Self, StoreError> {
        let path = path.as_ref().to_owned();
        tokio::task::spawn_blocking(move || -> Result<Self, StoreError> {
            let db = Db::new(path)?;
            Ok(Self { db: Arc::new(db) })
        })
        .await?
    }

    pub async fn min_decided_value_height(&self) -> Option<Height> {
        let db = Arc::clone(&self.db);
        tokio::task::spawn_blocking(move || db.min_decided_value_height())
            .await
            .ok()
            .flatten()
    }

    pub async fn max_decided_value_height(&self) -> Option<Height> {
        let db = Arc::clone(&self.db);
        tokio::task::spawn_blocking(move || db.max_decided_value_height())
            .await
            .ok()
            .flatten()
    }

    pub async fn get_decided_value(
        &self,
        height: Height,
    ) -> Result<Option<DecidedValue>, StoreError> {
        let db = Arc::clone(&self.db);
        tokio::task::spawn_blocking(move || db.get_decided_value(height)).await?
    }

    pub async fn store_decided_value(
        &self,
        certificate: &CommitCertificate<EthexeContext>,
        value: Value,
    ) -> Result<(), StoreError> {
        let dv = DecidedValue {
            value,
            certificate: certificate.clone(),
        };
        let db = Arc::clone(&self.db);
        tokio::task::spawn_blocking(move || db.insert_decided_value(dv)).await?
    }

    pub async fn store_undecided_proposal(
        &self,
        value: ProposedValue<EthexeContext>,
    ) -> Result<(), StoreError> {
        let db = Arc::clone(&self.db);
        tokio::task::spawn_blocking(move || db.insert_undecided_proposal(value)).await?
    }

    pub async fn get_undecided_proposal(
        &self,
        height: Height,
        round: Round,
        value_id: ValueId,
    ) -> Result<Option<ProposedValue<EthexeContext>>, StoreError> {
        let db = Arc::clone(&self.db);
        tokio::task::spawn_blocking(move || db.get_undecided_proposal(height, round, value_id))
            .await?
    }

    pub async fn get_undecided_proposals(
        &self,
        height: Height,
        round: Round,
    ) -> Result<Vec<ProposedValue<EthexeContext>>, StoreError> {
        let db = Arc::clone(&self.db);
        tokio::task::spawn_blocking(move || db.get_undecided_proposals(height, round)).await?
    }

    pub async fn store_pending_proposal_parts(
        &self,
        value: ProposalParts,
    ) -> Result<(), StoreError> {
        let db = Arc::clone(&self.db);
        tokio::task::spawn_blocking(move || db.insert_pending_proposal_parts(value)).await?
    }

    pub async fn get_pending_proposal_parts(
        &self,
        height: Height,
        round: Round,
    ) -> Result<Vec<ProposalParts>, StoreError> {
        let db = Arc::clone(&self.db);
        tokio::task::spawn_blocking(move || db.get_pending_proposal_parts(height, round)).await?
    }

    pub async fn remove_pending_proposal_parts(
        &self,
        value: ProposalParts,
    ) -> Result<(), StoreError> {
        let db = Arc::clone(&self.db);
        tokio::task::spawn_blocking(move || db.remove_pending_proposal_parts(value)).await?
    }

    pub async fn prune(
        &self,
        current_height: Height,
        retain_height: Height,
    ) -> Result<(), StoreError> {
        let db = Arc::clone(&self.db);
        tokio::task::spawn_blocking(move || db.prune(current_height, retain_height)).await?
    }

    pub async fn get_undecided_proposal_by_value_id(
        &self,
        value_id: ValueId,
    ) -> Result<Option<ProposedValue<EthexeContext>>, StoreError> {
        let db = Arc::clone(&self.db);
        tokio::task::spawn_blocking(move || db.get_undecided_proposal_by_value_id(value_id)).await?
    }
}
