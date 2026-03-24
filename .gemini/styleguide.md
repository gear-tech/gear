# Gear Gemini Review Style Guide

## Purpose

Review pull requests for the whole `gear` workspace with a balanced posture: prioritize correctness, safety-critical behavior, and verification gaps before maintainability comments.

## Review Priorities

1. Correctness and protocol behavior.
2. Safety-critical changes.
3. Verification gaps.
4. Maintainability only when it materially affects safety, debugging, or future regressions.

## Correctness And Protocol Behavior

Focus first on:

1. Logic errors and broken edge cases.
2. Invalid state transitions.
3. Event handling mistakes.
4. Ordering assumptions that can break under real execution.
5. Incorrect API, RPC, CLI, or tool usage.
6. Accidental behavior changes in runtime, protocol, batching, queueing, or externally visible flows.
7. Concurrency or race risks when code changes touch async, scheduling, or parallel processing behavior.

## Safety-Critical Changes

Treat these as high-attention areas:

1. Contract upgrades and migrations.
2. Storage compatibility.
3. Access control regressions.
4. Consensus-sensitive logic.
5. Validator, batching, or commitment limits.
6. ABI compatibility and source-to-generated artifact drift.
7. Changes that alter externally visible behavior without clear verification.

## Verification Expectations

Prefer comments about missing verification over comments about code style.

Look for:

1. Behavior changes without tests.
2. New invariants without CI or check coverage.
3. Source changes that appear to require regenerated ABI or artifact updates.
4. Deployment, workflow, or script changes without corresponding source-of-truth updates.
5. Workspace or toolchain changes without validation of cross-workspace effects.

## Anti-Noise Rules

Do not:

1. Comment on formatting already enforced by `rustfmt`, `forge fmt`, or repository linting.
2. Review generated JSON or ABI files directly.
3. Suggest broad refactors unless they have a clear correctness, verification, or maintenance benefit.
4. Flood the pull request with many small comments when one high-signal comment is enough.
5. Focus on naming, wording, or docs style unless the change makes behavior misleading or incomplete.
6. Present speculative concerns as findings without tying them to changed code.

## Repository-Specific Cues

1. Prefer reviewing source-of-truth files over generated artifacts.
2. If `ethexe/contracts/src/` changes, consider whether tests, ABI files, scripts, or relevant README instructions should also change.
3. If `.github/workflows/` changes, focus on weakened enforcement, reduced coverage, or accidental bypasses.
4. If `Cargo.toml`, `rust-toolchain.toml`, or workspace patches change, focus on toolchain alignment, version pinning, and cross-workspace effects.
5. If protocol behavior changes, expect deterministic verification paths and concrete evidence rather than reasoning alone.
6. Prefer comments on missing tests or missing regeneration steps over comments on superficial code organization.

## Generated Files

Generated files are not primary review targets. It is acceptable to comment that a source-of-truth change appears to require regenerated artifacts or committed outputs, but do not review the generated files themselves for formatting or style.
