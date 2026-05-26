// Copyright (C) Gear Technologies Inc.
// SPDX-License-Identifier: GPL-3.0-or-later WITH Classpath-exception-2.0

//! End-to-end integration tests for `ethexe-malachite-core`.
//!
//! Each test boots a fixed-size validator set on `127.0.0.1`, drives
//! the engines for a fixed wall-clock budget, and asserts that the
//! [`Externalities`] callbacks land in the contractual order
//! (`process_mb_proposal` strictly before `process_mb_finalized`
//! on the finalized chain).
//!
//! Tests are gated behind `#[tokio::test(flavor = "multi_thread")]`
//! because the malachite libp2p stack assumes a multi-thread runtime.

use std::{
    collections::HashMap,
    net::{SocketAddr, TcpListener},
    sync::{Arc, Mutex, Once},
    time::Duration,
};

fn init_tracing() {
    static ONCE: Once = Once::new();
    ONCE.call_once(|| {
        let _ = tracing_subscriber::fmt()
            .with_env_filter(
                tracing_subscriber::EnvFilter::try_from_default_env().unwrap_or_else(|_| {
                    tracing_subscriber::EnvFilter::new("warn,ethexe_malachite_core=info")
                }),
            )
            .with_test_writer()
            .try_init();
    });
}

use anyhow::Result;
use async_trait::async_trait;
use ethexe_malachite_core::{
    Block, CommitCertificate, Externalities, H256, MalachiteConfig, MalachiteService, Multiaddr,
    NodeRole, ValidatorEntry, libp2p_peer_id,
};
use parity_scale_codec::{Decode, Encode};
use proptest::prelude::*;
use tempfile::TempDir;
use tokio::time::sleep;

// --------------------------------------------------------------------
// TestPayload — minimal block payload type.
// `BlockPayload` is satisfied by the blanket impl, so no manual
// implementation needed.
// --------------------------------------------------------------------

#[derive(Clone, Debug, Encode, Decode, PartialEq, Eq)]
struct TestPayload {
    nonce: u64,
}

// --------------------------------------------------------------------
// TestExt — records every process_mb_proposal / process_mb_finalized
// call AND validates each `Externalities` contract guarantee in-line.
// Every violation gets pushed into `state.violations`; tests assert
// the vector is empty at the end.
//
// The contract checks (per the docs on `Externalities`):
//
// * `process_mb_proposal(hash, block)`:
//     - `hash == block.hash()`;
//     - `block.parent_hash` is `H256::zero()` (genesis) OR a
//       previously-seen `process_mb_proposal` hash (cascade_save
//       guarantees ancestor-first ordering);
//     - the same `hash` is never delivered twice.
//   Sibling proposals at the same height ARE allowed: when a round
//   times out and a new proposer steps in, the local node sees one
//   `process_mb_proposal` per assembled proposal, only one of which
//   ultimately finalizes.
// * `process_mb_finalized(hash, cert)`:
//     - `cert.block_hash == hash`;
//     - the matching block was previously processed as a proposal;
//     - finalize chain is gap-free and matches each block's
//       `parent_hash` chain (we finalize a strict linear sequence —
//       no sibling finalizes possible by construction).
// * `build_block_above(parent_hash)` / `validate_block_above(block)`:
//     - `parent_hash` (or `block.parent_hash`) equals our last
//       finalized block (or zero if we haven't seen any finalize yet —
//       fresh `TestExt` on a restarted node, or genesis).
//
// The same `Arc<TestExt>` may be reused across service restarts on
// the same home dir; the contract checks accumulate.
// --------------------------------------------------------------------

#[derive(Default)]
struct TestState {
    saved_blocks: HashMap<H256, Block<TestPayload>>,
    /// Linear finalized chain, ordered by `process_mb_finalized`
    /// arrival. Used to check gap-free finalize ordering and to
    /// detect duplicates.
    finalized: Vec<H256>,
    violations: Vec<String>,
}

#[derive(Default)]
struct TestExt {
    state: Mutex<TestState>,
}

impl TestExt {
    fn finalized_count(&self) -> usize {
        self.state.lock().unwrap().finalized.len()
    }

    fn violations(&self) -> Vec<String> {
        self.state.lock().unwrap().violations.clone()
    }
}

#[async_trait]
impl Externalities<TestPayload> for TestExt {
    async fn process_mb_proposal(&self, hash: H256, block: Block<TestPayload>) -> Result<()> {
        let mut s = self.state.lock().unwrap();
        if block.hash() != hash {
            s.violations
                .push("process_mb_proposal: hash arg does not match block.hash()".into());
        }
        if block.parent_hash != H256::zero() && !s.saved_blocks.contains_key(&block.parent_hash) {
            s.violations.push(format!(
                "process_mb_proposal: parent_hash {:?} not previously saved (cascade_save \
                 ancestor-first invariant violated)",
                block.parent_hash
            ));
        }
        if s.saved_blocks.contains_key(&hash) {
            s.violations
                .push(format!("process_mb_proposal: duplicate hash {hash:?}"));
        }
        s.saved_blocks.insert(hash, block);
        Ok(())
    }

    async fn process_mb_finalized(&self, hash: H256, cert: CommitCertificate) -> Result<()> {
        let mut s = self.state.lock().unwrap();
        if cert.block_hash != hash {
            s.violations
                .push("process_mb_finalized: cert.block_hash != hash arg".into());
        }
        if s.finalized.contains(&hash) {
            s.violations
                .push(format!("process_mb_finalized: duplicate hash {hash:?}"));
        }
        let Some(block) = s.saved_blocks.get(&hash).cloned() else {
            s.violations.push(format!(
                "process_mb_finalized: block {hash:?} was never delivered as a proposal"
            ));
            s.finalized.push(hash);
            return Ok(());
        };
        if cert.height != block.height {
            s.violations.push(format!(
                "process_mb_finalized: cert.height {} != saved height {}",
                cert.height, block.height
            ));
        }
        // Finalize chain must be linear and gap-free: each finalize's
        // parent_hash equals the previous finalize hash (or H256::zero
        // for the very first one).
        let expected_parent = s.finalized.last().copied().unwrap_or_else(H256::zero);
        if block.parent_hash != expected_parent {
            s.violations.push(format!(
                "process_mb_finalized: parent_hash mismatch — expected {:?}, got {:?}",
                expected_parent, block.parent_hash
            ));
        }
        s.finalized.push(hash);
        Ok(())
    }

    async fn build_block_above(&self, parent_hash: H256) -> Result<TestPayload> {
        let mut s = self.state.lock().unwrap();
        if let Some(last_fin) = s.finalized.last().copied()
            && parent_hash != last_fin
        {
            s.violations.push(format!(
                "build_block_above: parent_hash mismatch — expected {:?}, got {:?}",
                last_fin, parent_hash
            ));
        }
        Ok(TestPayload { nonce: 0 })
    }

    async fn validate_block_above(&self, parent_hash: H256, _payload: TestPayload) -> Result<bool> {
        let mut s = self.state.lock().unwrap();
        if let Some(last_fin) = s.finalized.last().copied()
            && parent_hash != last_fin
        {
            s.violations.push(format!(
                "validate_block_above: parent_hash mismatch — expected {last_fin:?}, got {parent_hash:?}"
            ));
        }
        Ok(true)
    }
}

// --------------------------------------------------------------------
// helpers — port allocation, validator setup, multiaddr assembly.
// --------------------------------------------------------------------

struct ValidatorSetup {
    private_key: gsigner::schemes::secp256k1::PrivateKey,
    home: TempDir,
    listen_addr: SocketAddr,
    peer_id: ethexe_malachite_core::PeerId,
}

fn make_secret(i: u16) -> [u8; 32] {
    // Spread the index over a wide range with a fixed-prefix tag so
    // every test secret is non-zero, distinct, and not adjacent to a
    // commonly-tried scalar.
    let mut s = [0u8; 32];
    s[0] = 0xa1;
    let bytes = i.to_be_bytes();
    s[30] = bytes[0];
    s[31] = bytes[1];
    s
}

fn make_validators(n: usize) -> Vec<ValidatorSetup> {
    // Bind every listener up front to grab a unique OS-assigned port,
    // then drop them so the engine can take over. This avoids
    // hardcoded port ranges that may already be in use.
    let listeners: Vec<TcpListener> = (0..n)
        .map(|_| TcpListener::bind("127.0.0.1:0").expect("bind 127.0.0.1:0"))
        .collect();
    let addrs: Vec<SocketAddr> = listeners
        .iter()
        .map(|l| l.local_addr().expect("local_addr"))
        .collect();
    drop(listeners);

    addrs
        .into_iter()
        .enumerate()
        .map(|(i, addr)| {
            let secret_bytes = make_secret(i as u16 + 1);
            let private_key = gsigner::schemes::secp256k1::PrivateKey::from_seed(secret_bytes)
                .expect("gsigner private key");
            let home = TempDir::new().expect("tempdir");
            let peer_id = libp2p_peer_id(&secret_bytes);
            ValidatorSetup {
                private_key,
                home,
                listen_addr: addr,
                peer_id,
            }
        })
        .collect()
}

fn validator_entries(setups: &[ValidatorSetup]) -> Vec<ValidatorEntry> {
    setups
        .iter()
        .map(|s| ValidatorEntry {
            public_key: s.private_key.public_key(),
            voting_power: 1,
        })
        .collect()
}

fn build_multiaddrs_excluding(setups: &[ValidatorSetup], exclude: usize) -> Vec<Multiaddr> {
    setups
        .iter()
        .enumerate()
        .filter(|(i, _)| *i != exclude)
        .map(|(_, s)| {
            let s = format!(
                "/ip4/127.0.0.1/tcp/{}/p2p/{}",
                s.listen_addr.port(),
                s.peer_id
            );
            s.parse().expect("multiaddr parses")
        })
        .collect()
}

fn build_config(
    setup: &ValidatorSetup,
    setups: &[ValidatorSetup],
    peers: Vec<Multiaddr>,
) -> MalachiteConfig {
    build_config_with_role(setup, peers, validator_entries(setups), NodeRole::Validator)
}

fn build_config_with_role(
    setup: &ValidatorSetup,
    peers: Vec<Multiaddr>,
    validators: Vec<ValidatorEntry>,
    role: NodeRole,
) -> MalachiteConfig {
    MalachiteConfig {
        listen_addr: setup.listen_addr,
        base: setup.home.path().to_path_buf(),
        persistent_peers: peers,
        validator_secret: setup.private_key.clone(),
        validators,
        propose_timeout: Duration::from_secs(2),
        role,
    }
}

async fn start_service(
    setup: &ValidatorSetup,
    setups: &[ValidatorSetup],
    idx: usize,
    ext: Arc<TestExt>,
) -> MalachiteService<TestPayload, TestExt> {
    let peers = build_multiaddrs_excluding(setups, idx);
    let config = build_config(setup, setups, peers);
    MalachiteService::<TestPayload, TestExt>::new(config, ext)
        .await
        .expect("service starts")
}

/// Wait until *every* validator has finalized at least `min_count`
/// blocks, or up to `budget` wall-clock has elapsed. Returns the
/// number of finalized blocks observed on the slowest validator.
async fn wait_for_finalized(exts: &[Arc<TestExt>], min_count: usize, budget: Duration) -> usize {
    let deadline = tokio::time::Instant::now() + budget;
    loop {
        let lo = exts.iter().map(|e| e.finalized_count()).min().unwrap_or(0);
        if lo >= min_count {
            return lo;
        }
        if tokio::time::Instant::now() >= deadline {
            return lo;
        }
        sleep(Duration::from_millis(200)).await;
    }
}

/// Per-validator contract assertion. The strict checks now live
/// inside [`TestExt`]; this helper just panics on any logged
/// violations.
fn assert_no_violations(name: &str, ext: &TestExt) {
    let viols = ext.violations();
    assert!(
        viols.is_empty(),
        "{name}: contract violations:\n  {}",
        viols.join("\n  ")
    );
}

// --------------------------------------------------------------------
// Tests
// --------------------------------------------------------------------

/// Three validators on a single host, no faults, runs for 25s. Every
/// validator must finalize at least three blocks in chronological
/// order.
#[tokio::test(flavor = "multi_thread", worker_threads = 6)]
async fn three_validators_make_progress() {
    init_tracing();
    let setups = make_validators(3);
    let exts: Vec<Arc<TestExt>> = (0..3).map(|_| Arc::new(TestExt::default())).collect();
    let mut services = Vec::with_capacity(3);
    for (i, setup) in setups.iter().enumerate() {
        let svc = start_service(setup, &setups, i, Arc::clone(&exts[i])).await;
        services.push(svc);
        // Stagger startup so validators don't all dial each other
        // simultaneously — concurrent dials produce two-way
        // connections which the malachite anti-spam treats as
        // duplicate proofs.
        sleep(Duration::from_millis(750)).await;
    }
    let lo = wait_for_finalized(&exts, 3, Duration::from_secs(90)).await;
    for svc in services {
        svc.shutdown().await;
    }
    assert!(lo >= 3, "slowest validator only finalized {lo}");
    for (i, ext) in exts.iter().enumerate() {
        assert_no_violations(&format!("v{i}"), ext);
    }
}

/// Seven validators, ~20 seconds of consensus, drop ALL services,
/// rebuild them on the same home dirs, run another ~20s. All
/// validators must continue from where they left off — finalized
/// heights must remain gap-free across the restart boundary.
#[tokio::test(flavor = "multi_thread", worker_threads = 8)]
async fn seven_validators_full_network_restart() {
    let setups = make_validators(7);
    // One Arc<TestExt> per validator slot — reused across the
    // restart so the contract checks accumulate.
    let exts: Vec<Arc<TestExt>> = (0..7).map(|_| Arc::new(TestExt::default())).collect();

    // ---- first run ------------------------------------------------
    let mut services = Vec::with_capacity(7);
    for (i, setup) in setups.iter().enumerate() {
        let svc = start_service(setup, &setups, i, Arc::clone(&exts[i])).await;
        services.push(svc);
    }
    sleep(Duration::from_secs(20)).await;
    let pre_finalized: Vec<usize> = exts.iter().map(|e| e.finalized_count()).collect();
    for svc in services {
        svc.shutdown().await;
    }

    // Give the OS a moment to release the listening sockets before
    // the second cohort comes up on the same home dirs. RocksDB
    // locks are released by `shutdown().await`; sockets need a
    // bit more.
    sleep(Duration::from_secs(2)).await;

    // ---- second run on the SAME home dirs -------------------------
    let mut services2 = Vec::with_capacity(7);
    for (i, setup) in setups.iter().enumerate() {
        let svc = start_service(setup, &setups, i, Arc::clone(&exts[i])).await;
        services2.push(svc);
    }
    // Wait for at least one validator to advance ≥ 1 height beyond
    // the pre-restart count.
    let target = pre_finalized.iter().min().copied().unwrap_or(0) + 1;
    let post_lo = wait_for_finalized(&exts, target, Duration::from_secs(60)).await;
    for svc in services2 {
        svc.shutdown().await;
    }

    for (i, c) in pre_finalized.iter().enumerate() {
        assert!(*c >= 1, "v{i} produced no finalized blocks before restart");
    }
    assert!(post_lo >= target, "no validator made post-restart progress");
    for (i, ext) in exts.iter().enumerate() {
        assert_no_violations(&format!("v{i}"), ext);
    }
}

/// One of the three validators is killed and rebuilt on the same
/// home dir mid-run; the network keeps making progress on the other
/// two, and the rejoiner must catch up.
#[tokio::test(flavor = "multi_thread", worker_threads = 6)]
async fn restart_one_validator_mid_run() {
    let setups = make_validators(3);

    let exts: Vec<Arc<TestExt>> = (0..3).map(|_| Arc::new(TestExt::default())).collect();
    let mut services: Vec<Option<MalachiteService<TestPayload, TestExt>>> = Vec::with_capacity(3);
    for (i, setup) in setups.iter().enumerate() {
        let svc = start_service(setup, &setups, i, Arc::clone(&exts[i])).await;
        services.push(Some(svc));
    }
    let _ = wait_for_finalized(&exts, 2, Duration::from_secs(45)).await;

    // Kill validator #2 and restart it on the same home dir. Use
    // `shutdown().await` to release the WAL/RocksDB locks before
    // starting again — `drop` is fire-and-forget. Reuse the same
    // `Arc<TestExt>` so the contract checks span the restart.
    if let Some(svc) = services[2].take() {
        svc.shutdown().await;
    }
    sleep(Duration::from_secs(2)).await;
    let pre_count = exts[2].finalized_count();
    let restarted = start_service(&setups[2], &setups, 2, Arc::clone(&exts[2])).await;
    services[2] = Some(restarted);

    let _ = wait_for_finalized(
        &[Arc::clone(&exts[2])],
        pre_count + 1,
        Duration::from_secs(45),
    )
    .await;
    for svc in services.into_iter().flatten() {
        svc.shutdown().await;
    }

    for (i, ext) in exts.iter().enumerate() {
        assert_no_violations(&format!("v{i}"), ext);
    }
    assert!(
        exts[2].finalized_count() > pre_count,
        "rejoined validator made no post-restart progress"
    );
}

/// Three validators run consensus; one full-node sits on the side.
/// The full-node must learn each finalized block via the
/// `process_mb_proposal` / `process_mb_finalized` callbacks
/// (delivered through the sync path) without ever signing a vote.
#[tokio::test(flavor = "multi_thread", worker_threads = 8)]
async fn full_node_syncs_from_validators() {
    let setups = make_validators(4);
    let validator_set: Vec<ValidatorEntry> = setups[..3]
        .iter()
        .map(|s| ValidatorEntry {
            public_key: s.private_key.public_key(),
            voting_power: 1,
        })
        .collect();

    let exts: Vec<Arc<TestExt>> = (0..4).map(|_| Arc::new(TestExt::default())).collect();
    let mut services = Vec::with_capacity(4);
    for (i, setup) in setups.iter().enumerate() {
        let role = if i < 3 {
            NodeRole::Validator
        } else {
            NodeRole::FullNode
        };
        let peers = build_multiaddrs_excluding(&setups, i);
        let cfg = build_config_with_role(setup, peers, validator_set.clone(), role);
        let svc = MalachiteService::<TestPayload, TestExt>::new(cfg, Arc::clone(&exts[i]))
            .await
            .expect("service starts");
        services.push(svc);
        sleep(Duration::from_millis(500)).await;
    }

    // Wait for the full-node to observe ≥ 3 finalize callbacks.
    let full_node_ext = Arc::clone(&exts[3]);
    let lo = wait_for_finalized(&[full_node_ext], 3, Duration::from_secs(90)).await;
    for svc in services {
        svc.shutdown().await;
    }
    assert!(lo >= 3, "full-node only finalized {lo}");

    assert_no_violations("fn", &exts[3]);

    // Each validator should also have made progress.
    for (i, ext) in exts[..3].iter().enumerate() {
        let count = ext.finalized_count();
        assert!(count >= 3, "validator {i} only finalized {count}");
    }
}

// --------------------------------------------------------------------
// Churn proptest: random kill/restart sequence on a 4-validator
// network. The strict checks inside [`TestExt`] catch any contract
// violation; this test fuzzes through scenarios to stress-exercise
// them under realistic timing.
// --------------------------------------------------------------------

#[derive(Clone, Debug)]
struct ChurnEvent {
    /// Wait this many milliseconds before applying the action.
    delay_ms: u64,
    /// `true` = kill the validator at `idx`; `false` = restart it.
    kill: bool,
    /// Validator slot to act on.
    idx: usize,
}

fn arb_churn_events(
    num_validators: usize,
    max_events: usize,
) -> impl Strategy<Value = Vec<ChurnEvent>> {
    let event = (1500u64..=3500u64, any::<bool>(), 0usize..num_validators).prop_map(
        |(delay_ms, kill, idx)| ChurnEvent {
            delay_ms,
            kill,
            idx,
        },
    );
    proptest::collection::vec(event, 0..=max_events)
}

fn run_churn_scenario(events: Vec<ChurnEvent>) {
    init_tracing();
    let rt = tokio::runtime::Builder::new_multi_thread()
        .worker_threads(8)
        .enable_all()
        .build()
        .expect("multi-thread runtime");
    rt.block_on(async move {
        let n = 4usize;
        // Tendermint quorum: >2/3 of voting power; with 4 equal-power
        // validators that's 3. We may kill only when alive > quorum.
        let quorum = 2 * n / 3 + 1;

        let setups = make_validators(n);
        let exts: Vec<Arc<TestExt>> = (0..n).map(|_| Arc::new(TestExt::default())).collect();
        let mut services: Vec<Option<MalachiteService<TestPayload, TestExt>>> =
            (0..n).map(|_| None).collect();

        // Bootstrap all validators with a stagger.
        for (i, setup) in setups.iter().enumerate() {
            services[i] = Some(start_service(setup, &setups, i, Arc::clone(&exts[i])).await);
            sleep(Duration::from_millis(500)).await;
        }
        // Let consensus run for a bit before applying churn.
        sleep(Duration::from_secs(3)).await;

        for ev in events {
            sleep(Duration::from_millis(ev.delay_ms)).await;
            let alive = services.iter().filter(|s| s.is_some()).count();
            if ev.kill {
                if services[ev.idx].is_some()
                    && alive > quorum
                    && let Some(svc) = services[ev.idx].take()
                {
                    svc.shutdown().await;
                }
            } else if services[ev.idx].is_none() {
                services[ev.idx] = Some(
                    start_service(&setups[ev.idx], &setups, ev.idx, Arc::clone(&exts[ev.idx]))
                        .await,
                );
            }
        }
        // Final settle window so the last surviving cohort can drain
        // any in-flight blocks.
        sleep(Duration::from_secs(5)).await;

        for svc in services.into_iter().flatten() {
            svc.shutdown().await;
        }

        for (i, ext) in exts.iter().enumerate() {
            assert_no_violations(&format!("v{i}"), ext);
        }
        let max_fin = exts.iter().map(|e| e.finalized_count()).max().unwrap_or(0);
        assert!(
            max_fin > 0,
            "no validator made any progress under churn (events: ?)"
        );
    });
}

proptest! {
    #![proptest_config(ProptestConfig {
        cases: 2,
        max_shrink_iters: 0,
        ..ProptestConfig::default()
    })]

    #[test]
    fn validator_churn_preserves_contracts(events in arb_churn_events(4, 6)) {
        run_churn_scenario(events);
    }
}
