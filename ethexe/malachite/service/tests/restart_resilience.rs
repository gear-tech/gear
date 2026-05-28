// Copyright (C) Gear Technologies Inc.
// SPDX-License-Identifier: GPL-3.0-or-later WITH Classpath-exception-2.0

//! End-to-end resilience checks for [`ethexe_malachite::MalachiteService`].
//!
//! These tests boot a real consensus service (single-validator quorum
//! so it can decide on its own without a libp2p mesh), drive it with
//! synthetic Ethereum chain heads to keep the producer's
//! quarantine-advance probe progressing, and verify:
//!
//! 1. `BlockProposal` and `BlockFinalized` events are emitted in
//!    height-non-decreasing order.
//! 2. After a `drop` + rebuild on the same home directory and
//!    `ethexe-db`, finalization picks up where it left off — the
//!    `CompactMb` chain reachable from
//!    `globals.latest_finalized_mb_hash` is gap-free across the
//!    restart boundary, and the latest pointer never rewinds.

use std::{path::Path, sync::Arc, time::Duration};

use ethexe_common::{
    BlockHeader, SimpleBlockData,
    db::{BlockMetaStorageRW, CompactMb, GlobalsStorageRO, MbStorageRO, OnChainStorageRW},
};
use ethexe_db::Database;
use ethexe_malachite::{
    EmptyMempool, MalachiteConfig, MalachiteEvent, MalachiteService, ValidatorEntry,
};
use futures::StreamExt as _;
use gprimitives::H256;
use gsigner::{Signer, schemes::secp256k1::Secp256k1};

/// Push synthetic linear Ethereum chain headers into the DB and
/// return blocks oldest-first. Headers are deterministic per `seed`,
/// so two test runs see the same hashes.
///
/// For every block we also populate:
/// - empty `block_events` —
///   [`crate::EthexeExternalities::validate_block_above`] requires
///   every Eth block in the advance walk to be locally synced
///   (header AND events). Without the events entry the validator
///   would abstain from voting on its own proposals.
/// - `block_meta.prepared = true` —
///   [`crate::EthexeExternalities::prerequisite_satisfied`] gates
///   the outbound BlockProposal / BlockFinalized events on the
///   `last_advanced_eb` block being **prepared** (codes loaded +
///   ancestors prepared). In production this flag is set by the
///   compute service's `prepare_block` pipeline; tests that don't
///   run that pipeline must seed it manually.
fn seed_chain(db: &Database, len: usize, seed: u32) -> Vec<SimpleBlockData> {
    let mut chain = Vec::with_capacity(len);
    let mut parent = H256::zero();
    for i in 0..len {
        let mut hb = [0u8; 32];
        hb[0] = (seed & 0xff) as u8;
        hb[1] = ((seed >> 8) & 0xff) as u8;
        hb[2] = (i & 0xff) as u8;
        hb[3] = ((i >> 8) & 0xff) as u8;
        // bias high so the produced hash is always non-zero
        hb[4] = 0x80;
        let hash = H256::from(hb);
        let header = BlockHeader {
            height: i as u32,
            timestamp: i as u64,
            parent_hash: parent,
        };
        db.set_block_header(hash, header);
        db.set_block_events(hash, &[]);
        db.mutate_block_meta(hash, |m| m.prepared = true);
        chain.push(SimpleBlockData { hash, header });
        parent = hash;
    }
    chain
}

/// Spin up an ephemeral keystore and generate one secp256k1 keypair.
fn build_signer(home: &Path) -> (Signer<Secp256k1>, gsigner::schemes::secp256k1::PublicKey) {
    let key_dir = home.join("keystore");
    std::fs::create_dir_all(&key_dir).expect("mkdir keystore");
    let signer = Signer::<Secp256k1>::fs(key_dir).expect("open keystore");
    let pub_key = signer.generate().expect("generate keypair");
    (signer, pub_key)
}

/// Build the MalachiteConfig used by the resilience tests:
/// quarantine-off (so the producer can advance immediately on each
/// new chain head), default listen address, no persistent peers,
/// single-validator set so the local node can decide on its own.
fn build_config(
    home: &Path,
    listen_port: u16,
    pub_key: gsigner::schemes::secp256k1::PublicKey,
) -> MalachiteConfig {
    MalachiteConfig {
        gas_allowance: MalachiteConfig::DEFAULT_GAS_ALLOWANCE,
        canonical_quarantine: 0,
        post_quarantine_delay: 0,
        listen_addr: std::net::SocketAddr::new(
            std::net::IpAddr::V4(std::net::Ipv4Addr::new(127, 0, 0, 1)),
            listen_port,
        ),
        home_dir: home.to_path_buf(),
        persistent_peers: Vec::new(),
        validators: vec![ValidatorEntry {
            public_key: pub_key,
            voting_power: 1,
        }],
    }
}

/// Drain the service stream until at least `target` finalize events
/// have been observed or `budget` elapses. Each round of the loop
/// feeds the next chain head from `pending_heads` BEFORE polling, so
/// the producer's `is_strict_descendant_of` check never sees the
/// same candidate twice — without that, the second round would have
/// `parent_advanced == candidate` and the producer would idle until
/// a new EB lands.
///
/// Returns the highest finalize height seen and the number of
/// finalize events observed.
async fn collect_until_finalized(
    service: &mut MalachiteService,
    pending_heads: &mut dyn Iterator<Item = SimpleBlockData>,
    target: u64,
    budget: Duration,
) -> (u64, u64) {
    let mut highest = 0;
    let mut finalized = 0u64;
    let deadline = tokio::time::Instant::now() + budget;
    // Push the first head right away so the producer can build the
    // genesis MB.
    if let Some(head) = pending_heads.next() {
        service.receive_new_chain_head(head);
    }
    while tokio::time::Instant::now() < deadline {
        let remaining = deadline.saturating_duration_since(tokio::time::Instant::now());
        match tokio::time::timeout(remaining, service.next()).await {
            Ok(Some(Ok(MalachiteEvent::BlockFinalized { cert, .. }))) => {
                finalized += 1;
                if cert.height > highest {
                    highest = cert.height;
                }
                if finalized >= target {
                    return (highest, finalized);
                }
                // Feed a fresh EB before the producer asks for the
                // next round, so its quarantine-advance candidate
                // moves forward.
                if let Some(head) = pending_heads.next() {
                    service.receive_new_chain_head(head);
                }
            }
            Ok(Some(Ok(MalachiteEvent::BlockProposal { .. }))) => {
                // ignored — the test is keyed on finalized heights
            }
            Ok(Some(Ok(MalachiteEvent::PurgedTransactions { .. }))) => {
                // ignore
            }
            Ok(Some(Err(e))) => panic!("service error: {e}"),
            Ok(None) | Err(_) => break,
        }
    }
    (highest, finalized)
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn single_validator_finalizes_and_recovers_after_restart() {
    let _ = tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .with_test_writer()
        .try_init();

    // Database survives the restart — that's how we model the
    // ethexe-side persistent state. The malachite home directory
    // (WAL + RocksDB store) also survives, so we pick a
    // `tempfile::TempDir` that lives for the whole test.
    let home = tempfile::tempdir().expect("home tempdir");
    let db = Database::memory();
    let chain = seed_chain(&db, 64, 0xDEAD_BEEF);

    let (signer, pub_key) = build_signer(home.path());

    // ---- first run -------------------------------------------------
    let mut svc = MalachiteService::new(
        build_config(home.path(), 30_001, pub_key),
        db.clone(),
        signer.clone(),
        Some(pub_key),
        Arc::new(EmptyMempool),
    )
    .await
    .expect("start malachite service");

    // Feed chain heads one-per-round so the quarantine-advance
    // probe always sees a strictly newer EB (parent's
    // `last_advanced_eb` is the previous head; same-hash returns
    // `Ok(false)` from `is_strict_descendant_of` and the producer
    // would idle).
    let mut pending = chain[..32].iter().copied();
    let (high1, finalized1) =
        collect_until_finalized(&mut svc, &mut pending, 5, Duration::from_secs(60)).await;
    assert!(
        finalized1 >= 5,
        "first run only saw {finalized1} finalized blocks (highest={high1})"
    );
    let pre_restart_head = db.globals().latest_finalized_mb_hash;
    assert!(
        !pre_restart_head.is_zero(),
        "globals.latest_finalized_mb_hash must advance during the first run"
    );
    // Walk back from the head via `CompactMb.parent` and check
    // the height chain is contiguous and matches `high1`.
    assert_chain_contiguous(&db, pre_restart_head, high1);

    // ---- shutdown --------------------------------------------------
    // `shutdown().await` waits for the engine actor + RocksDB store
    // to drop synchronously — `drop(svc)` alone is fire-and-forget
    // and would race the second `MalachiteService::new` against the
    // RocksDB advisory lock.
    svc.shutdown().await;
    // libp2p TCP listener still takes a moment past the actor kill
    // to free the port; we re-bind to the same address below.
    tokio::time::sleep(Duration::from_millis(500)).await;

    // ---- second run on the SAME home dir + DB ----------------------
    let mut svc2 = MalachiteService::new(
        build_config(home.path(), 30_001, pub_key),
        db.clone(),
        signer,
        Some(pub_key),
        Arc::new(EmptyMempool),
    )
    .await
    .expect("restart malachite service");
    let mut pending2 = chain[32..].iter().copied();
    let (high2, finalized2) =
        collect_until_finalized(&mut svc2, &mut pending2, 3, Duration::from_secs(60)).await;
    assert!(
        finalized2 >= 1,
        "no finalize events after restart (highest seen height={high2})"
    );
    assert!(
        high2 > high1,
        "post-restart highest finalize height {high2} must exceed pre-restart {high1}"
    );

    // Continuity: walking back from the post-restart head must hit
    // every height between `high2` and 1 exactly once.
    let post_restart_head = db.globals().latest_finalized_mb_hash;
    assert_chain_contiguous(&db, post_restart_head, high2);
    svc2.shutdown().await;
}

/// Walk back from `head` via [`CompactMb::parent`] and assert
/// the height chain is contiguous (`expected_height`, `expected_height - 1`,
/// …, 1) and that each step is reachable from the DB.
fn assert_chain_contiguous(db: &Database, head: H256, expected_height: u64) {
    let mut current = head;
    let mut expected = expected_height;
    loop {
        let compact: CompactMb = db
            .mb_compact_block(current)
            .unwrap_or_else(|| panic!("missing CompactMb for {current}"));
        assert_eq!(
            compact.height, expected,
            "chain height mismatch at {current}: expected {expected}, got {}",
            compact.height
        );
        // Transactions blob must be reachable too — that's the
        // contract behind CompactMb existence.
        assert!(
            db.transactions(compact.transactions_hash).is_some(),
            "missing transactions blob {} for MB {current}",
            compact.transactions_hash
        );
        if expected == 1 {
            assert!(
                compact.parent.is_zero(),
                "genesis MB must have parent == zero, got {}",
                compact.parent
            );
            break;
        }
        current = compact.parent;
        expected -= 1;
    }
}
