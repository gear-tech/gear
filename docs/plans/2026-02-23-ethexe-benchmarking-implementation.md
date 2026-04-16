# ETHEXE Consensus and Runtime Benchmarking Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Add a dedicated `ethexe` microbenchmark suite for consensus and wasm runtime (pure and host+wasm paths), runnable manually with a single script and documented usage.

**Architecture:** Create a new workspace crate `ethexe/benchmarks` powered by `criterion`, with deterministic fixture builders and three bench groups (`consensus`, `runtime_pure`, `runtime_host_wasm`). Add an orchestration script in `scripts/benchmarking/` that provides Gear-style execution ergonomics and consistent criterion options.

**Tech Stack:** Rust 2024, `criterion`, existing `ethexe-*` crates (`consensus`, `runtime-common`, `processor`, `db`, `common`), Bash runner script.

---

## Execution Notes

1. Skills to apply during execution:
   1. `@superpowers/test-driven-development`
   2. `@superpowers/verification-before-completion`
2. Worktree note:
   1. If a dedicated worktree is required, create it before Task 1.
3. Commit policy during execution:
   1. Use frequent commits for code changes.
   2. Do not include unrelated untracked files.

### Task 1: Bootstrap Benchmark Crate Skeleton

**Files:**

- Create: `/Users/ukintvs/Documents/projects/gear/ethexe/benchmarks/Cargo.toml`
- Create: `/Users/ukintvs/Documents/projects/gear/ethexe/benchmarks/src/lib.rs`
- Create: `/Users/ukintvs/Documents/projects/gear/ethexe/benchmarks/src/fixtures/mod.rs`
- Create: `/Users/ukintvs/Documents/projects/gear/ethexe/benchmarks/benches/consensus.rs`
- Create: `/Users/ukintvs/Documents/projects/gear/ethexe/benchmarks/benches/runtime_pure.rs`
- Create: `/Users/ukintvs/Documents/projects/gear/ethexe/benchmarks/benches/runtime_host_wasm.rs`

**Step 1: Write failing check (package missing)**

Run: `cargo check -p ethexe-benchmarks`
Expected: FAIL with package not found.

**Step 2: Add minimal crate manifest and bench entries**

```toml
[package]
name = "ethexe-benchmarks"
version.workspace = true
authors.workspace = true
edition.workspace = true
license.workspace = true
homepage.workspace = true
repository.workspace = true

[dependencies]
anyhow.workspace = true
ethexe-common.workspace = true
ethexe-consensus.workspace = true
ethexe-db.workspace = true
ethexe-processor.workspace = true
ethexe-runtime-common.workspace = true
gear-core.workspace = true
gprimitives.workspace = true
tokio.workspace = true

[dev-dependencies]
criterion = "0.5"

[[bench]]
name = "consensus"
harness = false

[[bench]]
name = "runtime_pure"
harness = false

[[bench]]
name = "runtime_host_wasm"
harness = false
```

**Step 3: Add no-op bench stubs**

```rust
use criterion::{Criterion, criterion_group, criterion_main};

fn bench_placeholder(c: &mut Criterion) {
    c.bench_function("placeholder", |b| b.iter(|| 1 + 1));
}

criterion_group!(benches, bench_placeholder);
criterion_main!(benches);
```

**Step 4: Verify crate compiles**

Run: `cargo check -p ethexe-benchmarks`
Expected: PASS.

**Step 5: Commit**

```bash
git add ethexe/benchmarks
git commit -m "feat(ethexe): scaffold benchmark crate"
```

### Task 2: Build Shared Deterministic Fixture Layer

**Files:**

- Create: `/Users/ukintvs/Documents/projects/gear/ethexe/benchmarks/src/fixtures/consensus.rs`
- Create: `/Users/ukintvs/Documents/projects/gear/ethexe/benchmarks/src/fixtures/runtime.rs`
- Create: `/Users/ukintvs/Documents/projects/gear/ethexe/benchmarks/src/fixtures/processor.rs`
- Modify: `/Users/ukintvs/Documents/projects/gear/ethexe/benchmarks/src/fixtures/mod.rs`
- Modify: `/Users/ukintvs/Documents/projects/gear/ethexe/benchmarks/src/lib.rs`

**Step 1: Add failing unit tests for fixture determinism**

```rust
#[test]
fn consensus_fixture_is_deterministic() {
    let a = build_consensus_fixture(64, 8);
    let b = build_consensus_fixture(64, 8);
    assert_eq!(a.signature, b.signature);
}
```

Run: `cargo test -p ethexe-benchmarks fixtures:: -- --nocapture`
Expected: FAIL with missing fixture functions.

**Step 2: Implement fixture structs and builders**

```rust
pub struct ConsensusFixture {
    pub db: Database,
    pub head: H256,
    pub commitment_delay_limit: u32,
    pub signature: (u32, u32),
}

pub fn build_consensus_fixture(blocks: u32, cdl: u32) -> ConsensusFixture {
    let db = Database::memory();
    let chain = BlockChain::mock(blocks).setup(&db);
    let head = chain.blocks[blocks as usize].hash;
    ConsensusFixture {
        db,
        head,
        commitment_delay_limit: cdl,
        signature: (blocks, cdl),
    }
}
```

**Step 3: Verify fixture tests pass**

Run: `cargo test -p ethexe-benchmarks fixtures::`
Expected: PASS.

**Step 4: Commit**

```bash
git add ethexe/benchmarks/src/lib.rs ethexe/benchmarks/src/fixtures
git commit -m "feat(ethexe-bench): add deterministic benchmark fixtures"
```

### Task 3: Implement Consensus Benchmarks

**Files:**

- Modify: `/Users/ukintvs/Documents/projects/gear/ethexe/benchmarks/benches/consensus.rs`
- Modify: `/Users/ukintvs/Documents/projects/gear/ethexe/benchmarks/src/fixtures/consensus.rs`

**Step 1: Write failing bench compile check for target functions**

Add references to:

1. `ethexe_consensus::announces::propagate_announces`
2. `ethexe_consensus::announces::accept_announce`
3. `ethexe_consensus::utils::try_aggregate_chain_commitment`
4. `ethexe_consensus::utils::calculate_batch_expiry`

Run: `cargo bench -p ethexe-benchmarks --bench consensus --no-run`
Expected: FAIL until imports/fixtures are complete.

**Step 2: Implement latency + scaling benchmark groups**

```rust
fn bench_chain_commitment(c: &mut Criterion) {
    let mut group = c.benchmark_group("consensus/chain_commitment");
    for blocks in [32_u32, 128, 512] {
        group.bench_with_input(BenchmarkId::from_parameter(blocks), &blocks, |b, &blocks| {
            b.iter_batched(
                || build_chain_commitment_case(blocks),
                |case| {
                    let _ = try_aggregate_chain_commitment(&case.db, case.block_hash, case.head);
                },
                BatchSize::LargeInput,
            )
        });
    }
    group.finish();
}
```

**Step 3: Compile benchmark target**

Run: `cargo bench -p ethexe-benchmarks --bench consensus --no-run`
Expected: PASS.

**Step 4: Smoke run benchmark**

Run: `cargo bench -p ethexe-benchmarks --bench consensus -- --noplot --sample-size 10`
Expected: PASS with terminal criterion report.

**Step 5: Commit**

```bash
git add ethexe/benchmarks/benches/consensus.rs ethexe/benchmarks/src/fixtures/consensus.rs
git commit -m "feat(ethexe-bench): add consensus microbenchmarks"
```

### Task 4: Implement Runtime-Pure Benchmarks

**Files:**

- Modify: `/Users/ukintvs/Documents/projects/gear/ethexe/benchmarks/benches/runtime_pure.rs`
- Modify: `/Users/ukintvs/Documents/projects/gear/ethexe/benchmarks/src/fixtures/runtime.rs`
- (If needed) Modify: `/Users/ukintvs/Documents/projects/gear/ethexe/runtime/common/src/lib.rs` (only if public helper exposure is required for bench integration)

**Step 1: Add failing compile reference to `process_queue` path**

Run: `cargo bench -p ethexe-benchmarks --bench runtime_pure --no-run`
Expected: FAIL until runtime fixture interface wiring is complete.

**Step 2: Implement runtime-pure fixture and benchmark cases**

```rust
fn bench_runtime_queue(c: &mut Criterion) {
    let mut group = c.benchmark_group("runtime/pure/process_queue");
    for queue_len in [1_usize, 8, 32] {
        group.bench_with_input(BenchmarkId::from_parameter(queue_len), &queue_len, |b, &n| {
            b.iter_batched(
                || build_runtime_queue_case(n),
                |case| {
                    let (_journals, _gas_spent) = ethexe_runtime_common::process_queue(case.ctx, &case.ri);
                },
                BatchSize::LargeInput,
            )
        });
    }
    group.finish();
}
```

**Step 3: Compile and smoke-run runtime_pure bench**

Run: `cargo bench -p ethexe-benchmarks --bench runtime_pure -- --noplot --sample-size 10`
Expected: PASS.

**Step 4: Commit**

```bash
git add ethexe/benchmarks/benches/runtime_pure.rs ethexe/benchmarks/src/fixtures/runtime.rs ethexe/runtime/common/src/lib.rs
git commit -m "feat(ethexe-bench): add runtime pure microbenchmarks"
```

### Task 5: Implement Host+wasm Runtime Benchmarks

**Files:**

- Modify: `/Users/ukintvs/Documents/projects/gear/ethexe/benchmarks/benches/runtime_host_wasm.rs`
- Modify: `/Users/ukintvs/Documents/projects/gear/ethexe/benchmarks/src/fixtures/processor.rs`

**Step 1: Write failing compile check for processor scenarios**

Run: `cargo bench -p ethexe-benchmarks --bench runtime_host_wasm --no-run`
Expected: FAIL until fixture assembly is complete.

**Step 2: Implement processor-based benchmark scenarios**

```rust
fn bench_process_code(c: &mut Criterion) {
    c.bench_function("runtime/host_wasm/process_code/small", |b| {
        b.iter_batched(
            build_small_valid_wasm_case,
            |mut case| {
                let _ = case.processor.process_code(case.code_and_id).unwrap();
            },
            BatchSize::LargeInput,
        )
    });
}
```

```rust
fn bench_process_programs(c: &mut Criterion) {
    let rt = tokio::runtime::Runtime::new().unwrap();
    c.bench_function("runtime/host_wasm/process_programs/medium", |b| {
        b.iter_batched(
            build_programs_case_medium,
            |mut case| {
                rt.block_on(async {
                    let _ = case.processor.process_programs(case.executable).await.unwrap();
                });
            },
            BatchSize::LargeInput,
        )
    });
}
```

**Step 3: Compile and smoke-run benchmark**

Run: `cargo bench -p ethexe-benchmarks --bench runtime_host_wasm -- --noplot --sample-size 10`
Expected: PASS.

**Step 4: Commit**

```bash
git add ethexe/benchmarks/benches/runtime_host_wasm.rs ethexe/benchmarks/src/fixtures/processor.rs
git commit -m "feat(ethexe-bench): add host+wasm runtime microbenchmarks"
```

### Task 6: Add Gear-Style Benchmark Runner Script

**Files:**

- Create: `/Users/ukintvs/Documents/projects/gear/scripts/benchmarking/run_ethexe_benchmarks.sh`

**Step 1: Write failing script invocation test**

Run: `bash scripts/benchmarking/run_ethexe_benchmarks.sh`
Expected: FAIL because script does not exist yet.

**Step 2: Implement script**

```bash
#!/usr/bin/env bash
set -euo pipefail

GROUP="${1:-all}"
COMMON_ARGS=(-- --noplot --sample-size "${SAMPLE_SIZE:-20}" --warm-up-time "${WARMUP_SECS:-2}" --measurement-time "${MEASURE_SECS:-5}")

run_bench() {
  local bench_name="$1"
  echo "[+] Running ethexe benchmark group: ${bench_name}"
  cargo bench -p ethexe-benchmarks --bench "${bench_name}" "${COMMON_ARGS[@]}"
}

case "${GROUP}" in
  all)
    run_bench consensus
    run_bench runtime_pure
    run_bench runtime_host_wasm
    ;;
  consensus|runtime_pure|runtime_host_wasm)
    run_bench "${GROUP}"
    ;;
  *)
    echo "Usage: $0 [all|consensus|runtime_pure|runtime_host_wasm]" >&2
    exit 1
    ;;
esac
```

**Step 3: Make script executable and verify**

Run: `chmod +x scripts/benchmarking/run_ethexe_benchmarks.sh`
Run: `scripts/benchmarking/run_ethexe_benchmarks.sh consensus`
Expected: PASS and benchmark output in terminal.

**Step 4: Commit**

```bash
git add scripts/benchmarking/run_ethexe_benchmarks.sh
git commit -m "chore(ethexe-bench): add benchmark orchestration script"
```

### Task 7: Update README with Usage and Manual Comparison

**Files:**

- Modify: `/Users/ukintvs/Documents/projects/gear/ethexe/README.md`

**Step 1: Add failing docs lint/readability check (manual)**

Run: `rg -n "Benchmarking" ethexe/README.md`
Expected: No dedicated section for new ethexe benchmark suite.

**Step 2: Add concise benchmark section**

```md
## Benchmarking

Run all ethexe benchmark groups:

```bash
scripts/benchmarking/run_ethexe_benchmarks.sh all
```

Run one group:

```bash
scripts/benchmarking/run_ethexe_benchmarks.sh consensus
```

Manual comparison:

1. Run the same group twice with identical env vars (`SAMPLE_SIZE`, `WARMUP_SECS`, `MEASURE_SECS`).
2. Compare median/mean reported by criterion in terminal output.
3. Treat single-run spikes as noise; rely on repeated runs.
```
```

**Step 3: Verify README includes section**

Run: `rg -n "Benchmarking|run_ethexe_benchmarks" ethexe/README.md`
Expected: PASS with matching lines.

**Step 4: Commit**

```bash
git add ethexe/README.md
git commit -m "docs(ethexe): document benchmark execution and comparison"
```

### Task 8: End-to-End Verification

**Files:**

- Verify only:
  1. `/Users/ukintvs/Documents/projects/gear/ethexe/benchmarks/**`
  2. `/Users/ukintvs/Documents/projects/gear/scripts/benchmarking/run_ethexe_benchmarks.sh`
  3. `/Users/ukintvs/Documents/projects/gear/ethexe/README.md`

**Step 1: Compile all benches**

Run: `cargo bench -p ethexe-benchmarks --no-run`
Expected: PASS.

**Step 2: Smoke-run all groups through script**

Run: `scripts/benchmarking/run_ethexe_benchmarks.sh all`
Expected: PASS with terminal outputs for three groups.

**Step 3: Final workspace sanity**

Run: `cargo check -p ethexe-benchmarks`
Expected: PASS.

**Step 4: Commit verification-only adjustments**

```bash
git add -A
git commit -m "test(ethexe-bench): verify benchmark suite end-to-end"
```
