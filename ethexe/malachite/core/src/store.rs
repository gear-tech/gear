// Copyright (C) Gear Technologies Inc.
// SPDX-License-Identifier: GPL-3.0-or-later WITH Classpath-exception-2.0

//! Persistent store: tracks every block the service is aware of
//! together with its `saved` / `finalized` flags, plus the chain-walk
//! algorithms that drive the strict ordering invariants documented on
//! [`crate::Externalities`].
//!
//! Storage is RocksDB, opened under `<base>/malachite/store.db`. The
//! key space is partitioned by a 1-byte prefix:
//!
//! - `0x01` `block_hash[32]`  → SCALE-encoded [`BlockEntry`]
//! - `0x02` `parent_hash[32]` → SCALE-encoded `Vec<H256>` (children)
//! - `0x03` `height_be[8]`    → SCALE-encoded `H256` (only finalized)
//! - `0x04` `meta_name`       → meta values (e.g. latest finalized)
//! - `0x05` `(height,round,value_id)` → engine undecided proposal
//! - `0x06` `(height,round,value_id)` → buffered proposal parts
//! - `0x07` `height_be[8]`    → engine-side `CommitCertificate`
//!
//! Children of the genesis (parent_hash == [`H256::zero`]) live under
//! the bare-zero parent key — same shape as any other parent.
//!
//! Algorithms ([`Store::save_chain`], [`Store::finalize_chain`])
//! return chronological-order chains *without* mutating the store —
//! the caller is expected to drive the application callback for each
//! entry and follow up with [`Store::mark_saved`] /
//! [`Store::mark_finalized`]. The cascade-from-children logic
//! (a block becoming saveable unblocks its descendants) is in
//! [`Store::cascade_save`] / [`Store::cascade_finalize`].

use std::{marker::PhantomData, path::Path, sync::Arc};

use anyhow::{Context as _, Result, anyhow};
use derive_where::derive_where;
use parity_scale_codec::{Decode, Encode};
use rocksdb::{DB, Options, WriteBatch};

use crate::{
    context::Height,
    externalities::BlockPayload,
    types::{Block, CommitCertificate, H256},
};

mod prefix {
    pub const BLOCK: u8 = 0x01;
    pub const CHILDREN: u8 = 0x02;
    pub const HEIGHT_INDEX: u8 = 0x03;
    pub const META: u8 = 0x04;
    pub const UNDECIDED: u8 = 0x05;
    pub const PENDING_PARTS: u8 = 0x06;
    pub const ENGINE_CERT: u8 = 0x07;
}

const META_LATEST_FINALIZED: &[u8] = b"latest_finalized";

/// Single block record kept by the service.
#[derive_where(Clone)]
#[derive(Encode, Decode)]
pub(crate) struct BlockEntry<P: BlockPayload> {
    pub block_hash: H256,
    pub parent_hash: H256,
    pub height: u64,
    pub payload: P,
    pub reserved: [u8; 64],
    pub saved: bool,
    pub finalized: bool,
    pub cert: Option<CommitCertificate>,
}

impl<P: BlockPayload> BlockEntry<P> {
    /// Reconstruct the [`Block`] form expected by
    /// [`crate::Externalities::process_mb_proposal`] /
    /// [`crate::Externalities::validate_block_above`].
    pub fn block(&self) -> Block<P> {
        Block {
            parent_hash: self.parent_hash,
            height: self.height,
            payload: self.payload.clone(),
            reserved: self.reserved,
        }
    }
}

#[derive(Clone, Encode, Decode)]
struct LatestFinalized {
    height: u64,
    block_hash: H256,
}

/// RocksDB-backed store. Cheap to clone (`Arc<DB>` inside).
pub(crate) struct Store<P: BlockPayload> {
    db: Arc<DB>,
    _phantom: PhantomData<fn() -> P>,
}

impl<P: BlockPayload> Clone for Store<P> {
    fn clone(&self) -> Self {
        Self {
            db: Arc::clone(&self.db),
            _phantom: PhantomData,
        }
    }
}

impl<P: BlockPayload> Store<P> {
    /// Open (creating if missing) the RocksDB at `path`.
    pub fn open(path: &Path) -> Result<Self> {
        std::fs::create_dir_all(path).with_context(|| format!("creating store dir {path:?}"))?;
        let mut opts = Options::default();
        opts.create_if_missing(true);
        let db = DB::open(&opts, path).with_context(|| format!("opening rocksdb at {path:?}"))?;
        Ok(Self {
            db: Arc::new(db),
            _phantom: PhantomData,
        })
    }

    fn key_block(hash: H256) -> [u8; 33] {
        let mut k = [0u8; 33];
        k[0] = prefix::BLOCK;
        k[1..33].copy_from_slice(hash.as_bytes());
        k
    }

    fn key_children(parent: H256) -> [u8; 33] {
        let mut k = [0u8; 33];
        k[0] = prefix::CHILDREN;
        k[1..33].copy_from_slice(parent.as_bytes());
        k
    }

    fn key_height(height: u64) -> [u8; 9] {
        let mut k = [0u8; 9];
        k[0] = prefix::HEIGHT_INDEX;
        k[1..9].copy_from_slice(&height.to_be_bytes());
        k
    }

    fn key_meta(name: &[u8]) -> Vec<u8> {
        let mut k = Vec::with_capacity(1 + name.len());
        k.push(prefix::META);
        k.extend_from_slice(name);
        k
    }

    fn decode_one<T: Decode>(bytes: &[u8], what: &'static str) -> Result<T> {
        T::decode(&mut &bytes[..]).with_context(|| format!("decoding {what}"))
    }

    /// Idempotent insert. If the block is already in store, the
    /// existing entry is preserved; only an absent `cert` field is
    /// filled in from the new entry. The children index is updated
    /// only on first insert.
    pub fn insert_block(&self, entry: BlockEntry<P>) -> Result<()> {
        let key = Self::key_block(entry.block_hash);
        let prev_bytes = self.db.get(key).context("reading existing block entry")?;
        let prev = match prev_bytes {
            Some(b) => Some(Self::decode_one::<BlockEntry<P>>(
                &b,
                "previous block entry",
            )?),
            None => None,
        };

        let mut batch = WriteBatch::default();

        let to_store = match prev {
            Some(mut e) => {
                if e.cert.is_none() && entry.cert.is_some() {
                    e.cert = entry.cert.clone();
                }
                e
            }
            None => {
                let parent_key = Self::key_children(entry.parent_hash);
                let mut children: Vec<H256> =
                    match self.db.get(parent_key).context("reading children list")? {
                        Some(b) => Self::decode_one(&b, "children list")?,
                        None => Vec::new(),
                    };
                if !children.contains(&entry.block_hash) {
                    children.push(entry.block_hash);
                    batch.put(parent_key, children.encode());
                }
                entry.clone()
            }
        };

        batch.put(key, to_store.encode());
        self.db.write(batch).context("writing block insert batch")?;
        Ok(())
    }

    /// Read a block entry by hash.
    pub fn get_block(&self, block_hash: H256) -> Result<Option<BlockEntry<P>>> {
        match self.db.get(Self::key_block(block_hash))? {
            Some(b) => Ok(Some(Self::decode_one(&b, "block entry")?)),
            None => Ok(None),
        }
    }

    /// Mark the block as `saved`. Idempotent. Errors if the block
    /// isn't in the store yet — the caller must `insert_block` first.
    pub fn mark_saved(&self, block_hash: H256) -> Result<()> {
        let mut entry = self
            .get_block(block_hash)?
            .ok_or_else(|| anyhow!("mark_saved: block {block_hash:?} not in store"))?;
        if entry.saved {
            return Ok(());
        }
        entry.saved = true;
        self.db
            .put(Self::key_block(block_hash), entry.encode())
            .context("writing mark_saved")?;
        Ok(())
    }

    /// Mark the block as `finalized`. The block must already be saved
    /// (the strict ordering invariant). Updates the height index and
    /// the `latest_finalized` meta record. Idempotent.
    pub fn mark_finalized(&self, block_hash: H256, cert: CommitCertificate) -> Result<()> {
        let mut entry = self
            .get_block(block_hash)?
            .ok_or_else(|| anyhow!("mark_finalized: block {block_hash:?} not in store"))?;
        if entry.finalized {
            return Ok(());
        }
        if !entry.saved {
            return Err(anyhow!(
                "mark_finalized: block {block_hash:?} is not saved yet (invariant violation)"
            ));
        }
        let height = entry.height;
        entry.finalized = true;
        entry.cert = Some(cert);

        let mut batch = WriteBatch::default();
        batch.put(Self::key_block(block_hash), entry.encode());
        batch.put(Self::key_height(height), block_hash.encode());

        let prev_lf = match self.db.get(Self::key_meta(META_LATEST_FINALIZED))? {
            Some(b) => Some(Self::decode_one::<LatestFinalized>(&b, "latest_finalized")?),
            None => None,
        };
        if prev_lf.as_ref().is_none_or(|p| height > p.height) {
            batch.put(
                Self::key_meta(META_LATEST_FINALIZED),
                LatestFinalized { height, block_hash }.encode(),
            );
        }

        self.db
            .write(batch)
            .context("writing mark_finalized batch")?;
        Ok(())
    }

    /// Children currently registered under `parent_hash`. Children of
    /// the genesis live under the bare-zero parent (where
    /// `parent_hash == H256::zero()`).
    pub fn children_of(&self, parent_hash: H256) -> Result<Vec<H256>> {
        match self.db.get(Self::key_children(parent_hash))? {
            Some(b) => Self::decode_one(&b, "children list"),
            None => Ok(Vec::new()),
        }
    }

    /// Walk back through parents from `leaf_hash` collecting every
    /// ancestor that has not yet been saved. Returns `None` if the walk
    /// hits a block that is not in the store (i.e. the chain is
    /// incomplete and we must wait). Genesis (parent_hash ==
    /// `H256::zero()`) and a previously saved ancestor are valid stop
    /// points.
    ///
    /// The returned chain is in chronological order
    /// (oldest-first), ready for sequential `process_mb_proposal` calls.
    pub fn save_chain(&self, leaf_hash: H256) -> Result<Option<Vec<BlockEntry<P>>>> {
        let mut chain_rev: Vec<BlockEntry<P>> = Vec::new();
        let mut current = leaf_hash;
        loop {
            let entry = match self.get_block(current)? {
                Some(e) => e,
                None => return Ok(None),
            };
            if entry.saved {
                break;
            }
            let parent = entry.parent_hash;
            chain_rev.push(entry);
            if parent == H256::zero() {
                break;
            }
            current = parent;
        }
        chain_rev.reverse();
        Ok(Some(chain_rev))
    }

    /// Walk back collecting every ancestor that is `saved` but not yet
    /// `finalized` and has a quorum certificate attached. Returns
    /// `None` if any ancestor is missing from the store, lacks a cert,
    /// or hasn't been saved (the strict invariant: finalize requires
    /// save first).
    ///
    /// The returned chain is chronological order (oldest-first).
    pub fn finalize_chain(&self, leaf_hash: H256) -> Result<Option<Vec<BlockEntry<P>>>> {
        let mut chain_rev: Vec<BlockEntry<P>> = Vec::new();
        let mut current = leaf_hash;
        loop {
            let entry = match self.get_block(current)? {
                Some(e) => e,
                None => return Ok(None),
            };
            if entry.finalized {
                break;
            }
            if entry.cert.is_none() || !entry.saved {
                return Ok(None);
            }
            let parent = entry.parent_hash;
            chain_rev.push(entry);
            if parent == H256::zero() {
                break;
            }
            current = parent;
        }
        chain_rev.reverse();
        Ok(Some(chain_rev))
    }

    /// Highest finalized block (height + hash), if any.
    pub fn latest_finalized(&self) -> Result<Option<(u64, H256)>> {
        match self.db.get(Self::key_meta(META_LATEST_FINALIZED))? {
            Some(b) => {
                let lf: LatestFinalized = Self::decode_one(&b, "latest_finalized")?;
                Ok(Some((lf.height, lf.block_hash)))
            }
            None => Ok(None),
        }
    }

    /// Block hash finalized at the given height, if any.
    pub fn finalized_block_at(&self, height: u64) -> Result<Option<H256>> {
        match self.db.get(Self::key_height(height))? {
            Some(b) => Ok(Some(Self::decode_one(&b, "height index")?)),
            None => Ok(None),
        }
    }

    /// Drive the application's `process_mb_proposal` callback over
    /// every ancestor that is now ready, starting from each seed. Cascades
    /// to children: when a block becomes saveable, its descendants are
    /// re-tried so a chain that was waiting on a missing middle gets
    /// flushed once the gap closes.
    pub async fn cascade_save<F, Fut>(&self, seeds: Vec<H256>, mut save_fn: F) -> Result<()>
    where
        F: FnMut(H256, Block<P>) -> Fut,
        Fut: std::future::Future<Output = Result<()>>,
    {
        let mut to_try = seeds;
        while let Some(hash) = to_try.pop() {
            let chain = match self.save_chain(hash)? {
                Some(c) => c,
                None => continue,
            };
            for entry in chain {
                if entry.saved {
                    continue;
                }
                let block = entry.block();
                save_fn(entry.block_hash, block).await?;
                self.mark_saved(entry.block_hash)?;
                let children = self.children_of(entry.block_hash)?;
                to_try.extend(children);
            }
        }
        Ok(())
    }

    /// Same shape as [`Self::cascade_save`] but for finalization. The
    /// callback receives the cert alongside the block hash.
    pub async fn cascade_finalize<F, Fut>(&self, seeds: Vec<H256>, mut finalize_fn: F) -> Result<()>
    where
        F: FnMut(H256, CommitCertificate) -> Fut,
        Fut: std::future::Future<Output = Result<()>>,
    {
        let mut to_try = seeds;
        while let Some(hash) = to_try.pop() {
            let chain = match self.finalize_chain(hash)? {
                Some(c) => c,
                None => continue,
            };
            for entry in chain {
                if entry.finalized {
                    continue;
                }
                let cert = entry
                    .cert
                    .clone()
                    .expect("finalize_chain returned an entry without cert");
                finalize_fn(entry.block_hash, cert.clone()).await?;
                self.mark_finalized(entry.block_hash, cert)?;
                let children = self.children_of(entry.block_hash)?;
                to_try.extend(children);
            }
        }
        Ok(())
    }

    // ---------------------------------------------------------------
    // Malachite-engine-facing storage (undecided proposals,
    // pending parts, height bounds) — colocated here because both
    // halves share a single RocksDB.
    // ---------------------------------------------------------------

    fn key_undecided(
        height: Height,
        round: malachitebft_core_types::Round,
        value_id: &crate::context::ValueId,
    ) -> [u8; 49] {
        let mut k = [0u8; 49];
        k[0] = prefix::UNDECIDED;
        k[1..9].copy_from_slice(&height.as_u64().to_be_bytes());
        k[9..17].copy_from_slice(&encode_round(round));
        k[17..49].copy_from_slice(&value_id.0);
        k
    }

    fn key_pending(
        height: Height,
        round: malachitebft_core_types::Round,
        value_id: &crate::context::ValueId,
    ) -> [u8; 49] {
        let mut k = [0u8; 49];
        k[0] = prefix::PENDING_PARTS;
        k[1..9].copy_from_slice(&height.as_u64().to_be_bytes());
        k[9..17].copy_from_slice(&encode_round(round));
        k[17..49].copy_from_slice(&value_id.0);
        k
    }

    fn prefix_undecided_hr(height: Height, round: malachitebft_core_types::Round) -> [u8; 17] {
        let mut k = [0u8; 17];
        k[0] = prefix::UNDECIDED;
        k[1..9].copy_from_slice(&height.as_u64().to_be_bytes());
        k[9..17].copy_from_slice(&encode_round(round));
        k
    }

    fn prefix_pending_hr(height: Height, round: malachitebft_core_types::Round) -> [u8; 17] {
        let mut k = [0u8; 17];
        k[0] = prefix::PENDING_PARTS;
        k[1..9].copy_from_slice(&height.as_u64().to_be_bytes());
        k[9..17].copy_from_slice(&encode_round(round));
        k
    }

    fn iter_prefix(&self, prefix_bytes: &[u8]) -> impl Iterator<Item = (Vec<u8>, Vec<u8>)> + '_ {
        use rocksdb::{Direction, IteratorMode};
        let prefix_owned = prefix_bytes.to_vec();
        self.db
            .iterator(IteratorMode::From(&prefix_owned, Direction::Forward))
            .filter_map(Result::ok)
            .take_while(move |(k, _)| k.starts_with(&prefix_owned))
            .map(|(k, v)| (k.to_vec(), v.to_vec()))
    }

    fn decode_height_from_key(k: &[u8]) -> Option<Height> {
        if k.len() < 9 {
            return None;
        }
        let bytes: [u8; 8] = k[1..9].try_into().ok()?;
        Some(Height::new(u64::from_be_bytes(bytes)))
    }

    pub fn store_undecided_proposal(
        &self,
        p: &malachitebft_core_consensus::ProposedValue<crate::context::MalachiteCtx>,
    ) -> Result<()> {
        use malachitebft_core_types::Value as _;
        let key = Self::key_undecided(p.height, p.round, &p.value.id());
        let bytes = crate::codec::encode_proposed_value(p);
        self.db
            .put(key, bytes)
            .context("storing undecided proposal")?;
        Ok(())
    }

    pub fn get_undecided_proposal(
        &self,
        height: Height,
        round: malachitebft_core_types::Round,
        value_id: &crate::context::ValueId,
    ) -> Result<Option<malachitebft_core_consensus::ProposedValue<crate::context::MalachiteCtx>>>
    {
        let key = Self::key_undecided(height, round, value_id);
        match self.db.get(key)? {
            Some(b) => Ok(Some(
                crate::codec::decode_proposed_value(&b).context("decoding undecided proposal")?,
            )),
            None => Ok(None),
        }
    }

    pub fn get_undecided_proposals(
        &self,
        height: Height,
        round: malachitebft_core_types::Round,
    ) -> Result<Vec<malachitebft_core_consensus::ProposedValue<crate::context::MalachiteCtx>>> {
        let p = Self::prefix_undecided_hr(height, round);
        let mut out = Vec::new();
        for (_, v) in self.iter_prefix(&p) {
            out.push(
                crate::codec::decode_proposed_value(&v)
                    .context("decoding undecided proposal in iter")?,
            );
        }
        Ok(out)
    }

    pub fn get_undecided_proposal_by_value_id(
        &self,
        value_id: &crate::context::ValueId,
    ) -> Result<Option<malachitebft_core_consensus::ProposedValue<crate::context::MalachiteCtx>>>
    {
        use malachitebft_core_types::Value as _;
        for (_, v) in self.iter_prefix(&[prefix::UNDECIDED]) {
            let p =
                crate::codec::decode_proposed_value(&v).context("decoding undecided proposal")?;
            if p.value.id() == *value_id {
                return Ok(Some(p));
            }
        }
        Ok(None)
    }

    pub fn store_pending_proposal_parts(
        &self,
        parts: &crate::streaming::ProposalParts,
        value_id: &crate::context::ValueId,
    ) -> Result<()> {
        let key = Self::key_pending(parts.height, parts.round, value_id);
        let bytes = crate::codec::encode_proposal_parts(parts);
        self.db.put(key, bytes).context("storing pending parts")?;
        Ok(())
    }

    pub fn get_pending_proposal_parts(
        &self,
        height: Height,
        round: malachitebft_core_types::Round,
    ) -> Result<Vec<crate::streaming::ProposalParts>> {
        let p = Self::prefix_pending_hr(height, round);
        let mut out = Vec::new();
        for (_, v) in self.iter_prefix(&p) {
            out.push(crate::codec::decode_proposal_parts(&v).context("decoding pending parts")?);
        }
        Ok(out)
    }

    /// Lowest finalized height, scanning the height index.
    pub fn min_finalized_height(&self) -> Result<Option<u64>> {
        let mut min: Option<u64> = None;
        for (k, _) in self.iter_prefix(&[prefix::HEIGHT_INDEX]) {
            if let Some(h) = Self::decode_height_from_key(&k) {
                let h = h.as_u64();
                min = Some(min.map_or(h, |m| m.min(h)));
            }
        }
        Ok(min)
    }

    /// Highest finalized height — just reads `latest_finalized` meta.
    pub fn max_finalized_height(&self) -> Result<Option<u64>> {
        Ok(self.latest_finalized()?.map(|(h, _)| h))
    }

    fn key_engine_cert(height: u64) -> [u8; 9] {
        let mut k = [0u8; 9];
        k[0] = prefix::ENGINE_CERT;
        k[1..9].copy_from_slice(&height.to_be_bytes());
        k
    }

    /// Persist the engine-side `CommitCertificate` keyed by height.
    /// We keep both this rich cert (with per-signer addresses, used
    /// for serving sync responses) and the trimmed
    /// [`crate::CommitCertificate`] inside [`BlockEntry`] (handed to
    /// the application via [`crate::Externalities`]).
    pub fn store_engine_certificate(
        &self,
        height: u64,
        cert: &malachitebft_core_types::CommitCertificate<crate::context::MalachiteCtx>,
    ) -> Result<()> {
        let bytes = crate::codec::encode_commit_certificate(cert);
        self.db
            .put(Self::key_engine_cert(height), bytes)
            .context("storing engine cert")?;
        Ok(())
    }

    pub fn get_engine_certificate(
        &self,
        height: u64,
    ) -> Result<Option<malachitebft_core_types::CommitCertificate<crate::context::MalachiteCtx>>>
    {
        match self.db.get(Self::key_engine_cert(height))? {
            Some(b) => Ok(Some(
                crate::codec::decode_commit_certificate(&b).context("decoding engine cert")?,
            )),
            None => Ok(None),
        }
    }

    /// Drop undecided proposals and pending parts at or below
    /// `current_height`. We've already committed at this height, so
    /// nothing in the engine-state columns at heights ≤ it can still be
    /// reached.
    pub fn prune_engine_state(&self, current_height: u64) -> Result<()> {
        let mut to_delete: Vec<Vec<u8>> = Vec::new();
        for p in [&[prefix::UNDECIDED][..], &[prefix::PENDING_PARTS][..]] {
            for (k, _) in self.iter_prefix(p) {
                if let Some(h) = Self::decode_height_from_key(&k)
                    && h.as_u64() <= current_height
                {
                    to_delete.push(k);
                }
            }
        }
        for k in to_delete {
            self.db.delete(k).context("deleting pruned engine state")?;
        }
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn encode_round(round: malachitebft_core_types::Round) -> [u8; 8] {
    ((round.as_i64() as u64) ^ 0x8000_0000_0000_0000_u64).to_be_bytes()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::{CommitCertificate, H256};
    use parity_scale_codec::{Decode, Encode};
    use proptest::prelude::*;
    use std::sync::Mutex;
    use tempfile::TempDir;

    #[derive(Clone, Debug, Encode, Decode, PartialEq, Eq)]
    struct TestPayload(Vec<u8>);

    fn h(n: u64) -> H256 {
        H256::from_low_u64_be(n)
    }

    fn open_store() -> (TempDir, Store<TestPayload>) {
        let dir = TempDir::new().unwrap();
        let store = Store::<TestPayload>::open(dir.path()).unwrap();
        (dir, store)
    }

    fn mk_entry(block_hash: H256, parent_hash: H256, height: u64) -> BlockEntry<TestPayload> {
        BlockEntry::<TestPayload> {
            block_hash,
            parent_hash,
            height,
            payload: TestPayload(vec![]),
            reserved: [0u8; 64],
            saved: false,
            finalized: false,
            cert: None,
        }
    }

    fn mk_cert(height: u64, block_hash: H256) -> CommitCertificate {
        CommitCertificate {
            height,
            block_hash,
            signatures: vec![vec![0u8; 64]],
        }
    }

    fn block_on<F: std::future::Future>(f: F) -> F::Output {
        tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap()
            .block_on(f)
    }

    // --- basic round-trip ------------------------------------------------

    #[test]
    fn insert_and_get() {
        let (_d, store) = open_store();
        let e = mk_entry(h(1), H256::zero(), 1);
        store.insert_block(e.clone()).unwrap();
        let got = store.get_block(h(1)).unwrap().unwrap();
        assert_eq!(got.block_hash, h(1));
        assert_eq!(got.parent_hash, H256::zero());
        assert_eq!(got.height, 1);
        assert!(!got.saved);
        assert!(!got.finalized);
    }

    #[test]
    fn insert_is_idempotent_and_preserves_state() {
        let (_d, store) = open_store();
        let e = mk_entry(h(1), H256::zero(), 1);
        store.insert_block(e.clone()).unwrap();
        store.mark_saved(h(1)).unwrap();
        store.insert_block(e.clone()).unwrap();
        assert!(store.get_block(h(1)).unwrap().unwrap().saved);
    }

    #[test]
    fn re_insert_promotes_cert_when_present() {
        let (_d, store) = open_store();
        store.insert_block(mk_entry(h(1), H256::zero(), 1)).unwrap();
        let mut e = mk_entry(h(1), H256::zero(), 1);
        e.cert = Some(mk_cert(1, h(1)));
        store.insert_block(e).unwrap();
        assert!(store.get_block(h(1)).unwrap().unwrap().cert.is_some());
    }

    #[test]
    fn children_index_basic() {
        let (_d, store) = open_store();
        store.insert_block(mk_entry(h(1), H256::zero(), 1)).unwrap();
        store.insert_block(mk_entry(h(2), h(1), 2)).unwrap();
        store.insert_block(mk_entry(h(3), h(1), 2)).unwrap();
        let mut kids = store.children_of(h(1)).unwrap();
        kids.sort_by_key(|x| x.to_low_u64_be());
        assert_eq!(kids, vec![h(2), h(3)]);
        assert_eq!(store.children_of(H256::zero()).unwrap(), vec![h(1)]);
    }

    #[test]
    fn children_index_no_duplicates_on_reinsert() {
        let (_d, store) = open_store();
        store.insert_block(mk_entry(h(1), H256::zero(), 1)).unwrap();
        for _ in 0..3 {
            store.insert_block(mk_entry(h(2), h(1), 2)).unwrap();
        }
        assert_eq!(store.children_of(h(1)).unwrap(), vec![h(2)]);
    }

    // --- save_chain ------------------------------------------------------

    #[test]
    fn save_chain_full_from_genesis() {
        let (_d, store) = open_store();
        store.insert_block(mk_entry(h(1), H256::zero(), 1)).unwrap();
        store.insert_block(mk_entry(h(2), h(1), 2)).unwrap();
        store.insert_block(mk_entry(h(3), h(2), 3)).unwrap();

        let chain = store.save_chain(h(3)).unwrap().unwrap();
        let hashes: Vec<_> = chain.iter().map(|e| e.block_hash).collect();
        assert_eq!(hashes, vec![h(1), h(2), h(3)]);
    }

    #[test]
    fn save_chain_returns_none_on_missing_ancestor() {
        let (_d, store) = open_store();
        store.insert_block(mk_entry(h(3), h(2), 3)).unwrap();
        assert!(store.save_chain(h(3)).unwrap().is_none());
    }

    #[test]
    fn save_chain_stops_at_saved_ancestor() {
        let (_d, store) = open_store();
        store.insert_block(mk_entry(h(1), H256::zero(), 1)).unwrap();
        store.insert_block(mk_entry(h(2), h(1), 2)).unwrap();
        store.insert_block(mk_entry(h(3), h(2), 3)).unwrap();
        store.mark_saved(h(1)).unwrap();

        let chain = store.save_chain(h(3)).unwrap().unwrap();
        let hashes: Vec<_> = chain.iter().map(|e| e.block_hash).collect();
        assert_eq!(hashes, vec![h(2), h(3)]);
    }

    #[test]
    fn save_chain_empty_when_leaf_is_already_saved() {
        let (_d, store) = open_store();
        store.insert_block(mk_entry(h(1), H256::zero(), 1)).unwrap();
        store.mark_saved(h(1)).unwrap();
        assert!(store.save_chain(h(1)).unwrap().unwrap().is_empty());
    }

    // --- finalize_chain --------------------------------------------------

    #[test]
    fn finalize_chain_requires_certs_and_saved() {
        let (_d, store) = open_store();
        let mut e1 = mk_entry(h(1), H256::zero(), 1);
        e1.cert = Some(mk_cert(1, h(1)));
        store.insert_block(e1).unwrap();
        assert!(store.finalize_chain(h(1)).unwrap().is_none());

        store.mark_saved(h(1)).unwrap();
        let chain = store.finalize_chain(h(1)).unwrap().unwrap();
        assert_eq!(chain.len(), 1);
    }

    #[test]
    fn finalize_chain_walks_back_only_through_certified_saved() {
        let (_d, store) = open_store();
        for i in 1..=3u64 {
            let parent = if i == 1 { H256::zero() } else { h(i - 1) };
            let mut e = mk_entry(h(i), parent, i);
            e.cert = Some(mk_cert(i, h(i)));
            store.insert_block(e).unwrap();
            store.mark_saved(h(i)).unwrap();
        }
        let chain = store.finalize_chain(h(3)).unwrap().unwrap();
        let hashes: Vec<_> = chain.iter().map(|e| e.block_hash).collect();
        assert_eq!(hashes, vec![h(1), h(2), h(3)]);
    }

    // --- cascade_save ----------------------------------------------------

    #[test]
    fn cascade_save_full_chain_in_order() {
        let (_d, store) = open_store();
        for i in 1..=5u64 {
            let parent = if i == 1 { H256::zero() } else { h(i - 1) };
            store.insert_block(mk_entry(h(i), parent, i)).unwrap();
        }
        let calls = Mutex::new(Vec::<H256>::new());
        block_on(async {
            store
                .cascade_save(vec![h(5)], |hash, _block| {
                    calls.lock().unwrap().push(hash);
                    async { Ok(()) }
                })
                .await
                .unwrap();
        });
        let recorded: Vec<_> = calls.lock().unwrap().clone();
        assert_eq!(recorded, vec![h(1), h(2), h(3), h(4), h(5)]);
        for i in 1..=5u64 {
            assert!(store.get_block(h(i)).unwrap().unwrap().saved);
        }
    }

    #[test]
    fn cascade_save_advances_descendants_after_gap_fills() {
        let (_d, store) = open_store();
        store.insert_block(mk_entry(h(3), h(2), 3)).unwrap();
        store.insert_block(mk_entry(h(1), H256::zero(), 1)).unwrap();
        store.insert_block(mk_entry(h(2), h(1), 2)).unwrap();

        let calls = Mutex::new(Vec::<H256>::new());
        block_on(async {
            store
                .cascade_save(vec![h(3)], |hash, _b| {
                    calls.lock().unwrap().push(hash);
                    async { Ok(()) }
                })
                .await
                .unwrap();
        });
        assert_eq!(*calls.lock().unwrap(), vec![h(1), h(2), h(3)]);
    }

    #[test]
    fn cascade_save_is_noop_when_chain_incomplete() {
        let (_d, store) = open_store();
        store.insert_block(mk_entry(h(3), h(2), 3)).unwrap();
        let calls = Mutex::new(Vec::<H256>::new());
        block_on(async {
            store
                .cascade_save(vec![h(3)], |hash, _| {
                    calls.lock().unwrap().push(hash);
                    async { Ok(()) }
                })
                .await
                .unwrap();
        });
        assert!(calls.lock().unwrap().is_empty());
        assert!(!store.get_block(h(3)).unwrap().unwrap().saved);
    }

    #[test]
    fn cascade_save_unblocks_pending_descendants_when_seeded_from_root() {
        let (_d, store) = open_store();
        store.insert_block(mk_entry(h(3), h(2), 3)).unwrap();
        store.insert_block(mk_entry(h(2), h(1), 2)).unwrap();
        store.insert_block(mk_entry(h(1), H256::zero(), 1)).unwrap();
        let calls = Mutex::new(Vec::<H256>::new());
        block_on(async {
            store
                .cascade_save(vec![h(1)], |hash, _| {
                    calls.lock().unwrap().push(hash);
                    async { Ok(()) }
                })
                .await
                .unwrap();
        });
        assert_eq!(*calls.lock().unwrap(), vec![h(1), h(2), h(3)]);
    }

    #[test]
    fn cascade_finalize_in_strict_order() {
        let (_d, store) = open_store();
        for i in 1..=4u64 {
            let parent = if i == 1 { H256::zero() } else { h(i - 1) };
            let mut e = mk_entry(h(i), parent, i);
            e.cert = Some(mk_cert(i, h(i)));
            store.insert_block(e).unwrap();
            store.mark_saved(h(i)).unwrap();
        }
        let calls = Mutex::new(Vec::<H256>::new());
        block_on(async {
            store
                .cascade_finalize(vec![h(4)], |hash, _c| {
                    calls.lock().unwrap().push(hash);
                    async { Ok(()) }
                })
                .await
                .unwrap();
        });
        assert_eq!(*calls.lock().unwrap(), vec![h(1), h(2), h(3), h(4)]);
        let (height, hash) = store.latest_finalized().unwrap().unwrap();
        assert_eq!((height, hash), (4, h(4)));
        for i in 1..=4u64 {
            assert_eq!(store.finalized_block_at(i).unwrap(), Some(h(i)));
        }
    }

    #[test]
    fn mark_finalized_rejects_unsaved_block() {
        let (_d, store) = open_store();
        store.insert_block(mk_entry(h(1), H256::zero(), 1)).unwrap();
        let err = store.mark_finalized(h(1), mk_cert(1, h(1))).unwrap_err();
        assert!(err.to_string().contains("not saved yet"));
    }

    // --- restart persistence --------------------------------------------

    #[test]
    fn state_survives_reopen() {
        let dir = TempDir::new().unwrap();
        {
            let store = Store::<TestPayload>::open(dir.path()).unwrap();
            for i in 1..=3u64 {
                let parent = if i == 1 { H256::zero() } else { h(i - 1) };
                let mut e = mk_entry(h(i), parent, i);
                e.cert = Some(mk_cert(i, h(i)));
                store.insert_block(e).unwrap();
                store.mark_saved(h(i)).unwrap();
                store.mark_finalized(h(i), mk_cert(i, h(i))).unwrap();
            }
        }
        let store2 = Store::<TestPayload>::open(dir.path()).unwrap();
        assert_eq!(store2.latest_finalized().unwrap(), Some((3, h(3))));
        for i in 1..=3u64 {
            let e = store2.get_block(h(i)).unwrap().unwrap();
            assert!(e.saved && e.finalized);
        }
    }

    // --- proptest --------------------------------------------------------

    fn arb_chain_with_order(len: u64) -> impl Strategy<Value = (u64, Vec<usize>)> {
        let l = len as usize;
        Just(0)
            .prop_flat_map(move |_| proptest::collection::vec(any::<u32>(), l))
            .prop_map(move |seed| {
                let mut order: Vec<usize> = (0..l).collect();
                if order.len() > 1 {
                    for i in (1..order.len()).rev() {
                        let j = (seed[i] as usize) % (i + 1);
                        order.swap(i, j);
                    }
                }
                (len, order)
            })
    }

    proptest! {
        #![proptest_config(ProptestConfig::with_cases(32))]

        #[test]
        fn prop_save_chain_eventually_saves_all_in_order(
            (len, order) in (1u64..16).prop_flat_map(arb_chain_with_order)
        ) {
            let (_d, store) = open_store();
            for &idx in &order {
                let i = (idx as u64) + 1;
                let parent = if i == 1 { H256::zero() } else { h(i - 1) };
                store.insert_block(mk_entry(h(i), parent, i)).unwrap();
            }
            let calls = Mutex::new(Vec::<H256>::new());
            block_on(async {
                store
                    .cascade_save(vec![h(len)], |hash, _| {
                        calls.lock().unwrap().push(hash);
                        async { Ok(()) }
                    })
                    .await
                    .unwrap();
            });
            block_on(async {
                store
                    .cascade_save(vec![h(1)], |hash, _| {
                        calls.lock().unwrap().push(hash);
                        async { Ok(()) }
                    })
                    .await
                    .unwrap();
            });
            for i in 1..=len {
                let e = store.get_block(h(i)).unwrap().unwrap();
                prop_assert!(e.saved, "block {} not saved", i);
            }
            let recorded = calls.lock().unwrap().clone();
            for w in recorded.windows(2) {
                let a = w[0].to_low_u64_be();
                let b = w[1].to_low_u64_be();
                prop_assert!(a < b, "non-monotonic save order: {:?}", recorded);
            }
        }

        #[test]
        fn prop_save_chain_is_idempotent_under_repeated_cascades(
            len in 1u64..10
        ) {
            let (_d, store) = open_store();
            for i in 1..=len {
                let parent = if i == 1 { H256::zero() } else { h(i - 1) };
                store.insert_block(mk_entry(h(i), parent, i)).unwrap();
            }
            let calls = Mutex::new(Vec::<H256>::new());
            for _ in 0..5 {
                block_on(async {
                    store
                        .cascade_save(vec![h(len)], |hash, _| {
                            calls.lock().unwrap().push(hash);
                            async { Ok(()) }
                        })
                        .await
                        .unwrap();
                });
            }
            let recorded = calls.lock().unwrap().clone();
            prop_assert_eq!(recorded.len(), len as usize);
        }

        #[test]
        fn prop_finalize_after_save_keeps_strict_order(
            len in 1u64..10
        ) {
            let (_d, store) = open_store();
            for i in 1..=len {
                let parent = if i == 1 { H256::zero() } else { h(i - 1) };
                let mut e = mk_entry(h(i), parent, i);
                e.cert = Some(mk_cert(i, h(i)));
                store.insert_block(e).unwrap();
            }
            block_on(async {
                store
                    .cascade_save(vec![h(len)], |_, _| async { Ok(()) })
                    .await
                    .unwrap();
            });
            let calls = Mutex::new(Vec::<H256>::new());
            block_on(async {
                store
                    .cascade_finalize(vec![h(len)], |hash, _c| {
                        calls.lock().unwrap().push(hash);
                        async { Ok(()) }
                    })
                    .await
                    .unwrap();
            });
            let recorded = calls.lock().unwrap().clone();
            for i in 1..=len {
                prop_assert!(recorded.contains(&h(i)));
            }
            for w in recorded.windows(2) {
                let a = w[0].to_low_u64_be();
                let b = w[1].to_low_u64_be();
                prop_assert!(a < b, "non-monotonic finalize order: {:?}", recorded);
            }
        }
    }
}
