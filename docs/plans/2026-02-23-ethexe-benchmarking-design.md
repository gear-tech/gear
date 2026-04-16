# ETHEXE Consensus and Runtime Benchmarking Design

## Scope

Design and implement microbenchmarking for:

1. `ethexe` consensus hot paths.
2. wasm runtime execution paths in two layers:
   1. Pure runtime processing path.
   2. Host + wasm invocation path.

Out of scope:

1. FRAME runtime weight benchmarking integration.
2. CI gating (initial rollout is manual execution only).
3. Persisted benchmark artifacts (JSON/CSV files).

## Confirmed Decisions

1. Benchmark scope: microbenchmarks only.
2. Optimize for both:
   1. Fixed-size latency.
   2. Scalability curves.
3. CI policy: no gate for now (manual runs only).
4. Harness strategy: hybrid.
   1. `criterion` for measurement/statistics.
   2. Custom script for Gear-style orchestration.
5. Packaging model: dedicated benchmark crate (`ethexe/benchmarks`).
6. Result handling: terminal output only.
7. Definition of done:
   1. Runnable benchmark suite for agreed targets.
   2. README guidance for manual run comparison.

## Why Not Reuse FRAME Runtime Benchmarking

Existing Substrate/Gear benchmarking in this repository is built around:

1. FRAME pallet extrinsic benchmarking.
2. Runtime weight generation pipeline.

Consensus and most `ethexe` runtime internals are off-chain service/runtime internals, not pallet extrinsics. Reusing FRAME runtime benchmarking for this purpose would be forced and less maintainable than a dedicated microbenchmark crate.

## Architecture

## Components

1. New crate: `ethexe/benchmarks`.
2. Benchmark groups:
   1. `consensus`.
   2. `runtime_pure`.
   3. `runtime_host_wasm`.
3. Shared fixtures module in the benchmark crate for deterministic setup.
4. Runner script:
   1. `scripts/benchmarking/run_ethexe_benchmarks.sh`.
   2. Single entrypoint for running all or selected groups with consistent options.

## Benchmark Targets

### Consensus

Primary targets:

1. `announces::propagate_announces`.
2. `announces::accept_announce`.
3. `utils::try_aggregate_chain_commitment`.
4. `utils::calculate_batch_expiry`.

Scales:

1. Block chain length and branch complexity.
2. Announce depth relative to `commitment_delay_limit`.
3. Transition count per announce.
4. Injected transaction count where relevant.

### Runtime Pure

Primary target:

1. `ethexe_runtime_common::process_queue` using deterministic in-memory fixtures with runtime-interface wiring that excludes external network effects.

Scales:

1. Queue size (small, medium, large).
2. Message payload sizes.
3. Canonical vs injected queue path.

### Runtime Host + wasm

Primary targets:

1. `Processor::process_code` (instrumentation path).
2. `Processor::process_programs` / queue-processing flow through host+wasm boundary.

Scales:

1. wasm code size buckets.
2. Program count and queue depth.
3. Chunk size configuration.

## Data Flow

1. Operator runs one script command.
2. Script selects bench target(s) and forwards stable criterion options.
3. Each benchmark case:
   1. Builds deterministic fixture data.
   2. Measures only the target call.
   3. Prints criterion output to terminal.
4. No benchmark artifacts are written to repository-managed files.

## Stability and Quality Controls

1. Deterministic seeds for generated inputs.
2. Explicit separation of setup from timed region.
3. Fixed case naming and scale parameters.
4. No network/anvil/ethereum dependencies in microbench paths.
5. Smoke verification commands for each group before completion claims.

## Documentation Plan

Update `ethexe/README.md` with:

1. Available benchmark groups.
2. Run commands.
3. Manual comparison guidance between two runs.

## Risks and Mitigations

1. Runtime pure path may require additional fixture wiring for runtime interfaces.
   1. Mitigation: introduce minimal benchmark support wrapper in bench crate while reusing existing test setup patterns.
2. Noise from machine variability.
   1. Mitigation: standardize run flags via script and document recommended local conditions.
3. Overly broad first delivery.
   1. Mitigation: start with a minimal but representative scenario matrix and expand iteratively.

## Rollout

1. Add benchmark crate and first scenarios.
2. Add runner script and README instructions.
3. Validate local execution.
4. Expand scenario coverage in follow-up changes as needed.
