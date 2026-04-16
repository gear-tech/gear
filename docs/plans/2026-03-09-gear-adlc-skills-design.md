# Gear ADLC and Gear Skills RFC

**Status:** Draft

**Audience:** Gear Foundation maintainers, Sails maintainers, and contributors who will curate agent-facing workflows for Gear and Vara.

## Summary

This RFC defines two linked deliverables:

1. A Gear/Vara Agent Development Lifecycle (ADLC) built around deterministic local feedback loops instead of prompt-only reasoning.
2. A normative `Gear Skills` standard published from a separate `gear-skills` repository.

The goal is bounded autonomy, not unrestricted autonomy. Agents may draft, test, and debug against deterministic tools, but design approval and release remain human gates. In `v1`, `gear`, `sails`, and `awesome-sails` remain the source repositories for code, templates, and fixtures. The new `gear-skills` repository becomes the curated packaging and validation layer that teaches agents how to use those repositories safely.

This RFC deliberately follows the progressive-disclosure model popularized by `mgechev/skills-best-practices`: load the smallest possible unit of context first, then pull scripts, references, and assets only when the current step demands them.

## Decision Summary

- Adopt ADLC as the default maintainer-facing workflow for agentic Gear/Vara development.
- Publish curated skills from a separate `gear-skills` repository instead of embedding them into `gear`.
- Make the initial catalog Sails-first:
  `sails-new-program`, `gear-test-sails-program`, `gear-run-local-node`, `gear-deploy-program`, and `gear-query-program-state`.
- Treat `gtest` as the default off-chain verifier, `gcli` as the default CLI surface, and `gear-node-wrapper` as the default local-node orchestration primitive.
- Use `sails` for scaffolding/templates and `awesome-sails` for realistic validation fixtures.
- Change no existing Rust crate APIs in `v1`; this RFC adds documentation, repository contracts, and validation policy only.

## Problem Statement

AI agents are useful accelerators, but they are not deterministic components. They forget context, overgeneralize from partial examples, and fabricate missing procedural knowledge. Gear and Vara already expose strong deterministic surfaces, but those surfaces are not packaged in a way that lets external orchestrators use them consistently.

Current repository strengths:

- `gtest` is already the best pre-deploy verifier for Gear programs. Its current implementation uses `GasTreeManager` in `gtest/src/state/gas_tree.rs` and `IntervalsTree`-based allocation tracking in `gtest/src/manager/journal.rs`, which makes gas and memory failures concrete and reproducible before mainnet deployment.
- `gear-node-wrapper` is a real reusable primitive, not just an internal convenience crate. It exposes `Node` and `NodeInstance` in `utils/node-wrapper/src/lib.rs` and is already consumed by `gcli/tests/util.rs` and `gsdk/tests/utils/mod.rs`.
- `gcli` already provides a CLI surface for config, wallet setup, deploy, send, and state reads. Its public command set is documented in `gcli/src/lib.rs` and exercised in `gcli/tests/smoke.rs`.
- `sails` already provides a scaffolding and codegen surface through `cargo sails`, including `new`, `program`, `idl`, `client-rs`, and `sol` commands in `sails/rs/cli/src/main.rs`.
- `awesome-sails` already provides realistic Sails + `gtest` fixtures, including `/Users/ukintvs/Documents/projects/awesome-sails/tests/awesome-sails-test/app/tests/gtest.rs` and `/Users/ukintvs/Documents/projects/awesome-sails/tests/access-control-test/app/tests/gtest.rs`.

What is missing is a standard, validated layer that tells agents:

1. Which deterministic tools to prefer first.
2. Which repository paths are authoritative for each workflow.
3. How to load context incrementally instead of swallowing full repositories.
4. How to fail closed on unsupported targets such as EVM, Solidity, or non-WASM workflows.

## Part I: Gear Agent Development Lifecycle

### Design Principles

The Gear ADLC standard is defined by five rules:

1. Deterministic tools outrank model intuition.
2. Context is loaded just in time, not all at once.
3. Fragile protocol interactions must move into scripts or fixed workflows.
4. Every phase has an explicit gate and exit artifact.
5. Unsupported or ambiguous targets must abort, not improvise.

### Lifecycle Phases and Gates

| Phase | Purpose | Required outputs | Gate |
| --- | --- | --- | --- |
| Ideation and design | Define goal, audience, contract type, network target, safety constraints, and context map | Problem statement, success criteria, repo map, negative scope | Human approval before code generation |
| Implementation | Scaffold or modify code using approved sources | Code changes plus matching tests/docs | Every touched workflow maps to an existing API, template, or fixture |
| Deterministic local verification | Prove behavior off-chain before any live deployment step | `cargo test`, `gtest`, build, and IDL/client generation results | All required local checks succeed or fail with actionable diagnostics |
| Local node integration | Verify deploy/send/query behavior against a real local node | Dev-node logs, CLI output, deploy/query transcript | Local round-trip works or a deterministic failure is captured |
| Review and release | Confirm requirements and publish safely | Review notes, final diffs, release decision | Human sign-off |

### Phase Requirements

#### 1. Ideation and design

An agent must not begin implementation until it has a written design that names:

- the target program type (`Sails` program, plain Gear program, fixture update, or tooling change);
- the network scope (`gtest` only, local node, testnet, or mainnet);
- the success criteria;
- explicit negative scope; and
- the authoritative repositories and entrypoints it may use.

This phase exists because unscoped autonomy is the fastest path to hallucinated APIs and wrong deployment targets.

#### 2. Implementation

Implementation is constrained by existing sources of truth:

- scaffolding and generated examples come from `sails`;
- local protocol behavior comes from `gear`;
- reusable realistic fixtures come from `awesome-sails`.

Agents may compose those sources, but they should not invent new workflow steps when a template, example, or script already exists.

#### 3. Deterministic local verification

`gtest` is the default verifier for `v1`. A compliant ADLC flow must run deterministic local checks before any deploy step:

- build the program and generated artifacts;
- run `gtest`-based tests;
- capture stdout/stderr for compiler, test, and gas failures; and
- use those failures as the primary debugging loop.

If `gtest` reports a failure, gas exhaustion, or memory/allocation problem, the agent must treat that output as authoritative and iterate locally. It must not skip straight to live-node retries.

#### 4. Local node integration

Local node integration is required for any workflow that claims deploy, send, or query support. The default primitives are:

- `gear-node-wrapper` when the workflow needs a programmatic local node;
- `gear --dev` when a shell-driven node is sufficient; and
- `gcli` for config, wallet, deploy, send, and query operations.

The node integration phase exists to bridge the gap between deterministic off-chain verification and a live runtime with RPC behavior, endpoint configuration, and account setup.

#### 5. Review and release

Release is deliberately non-agentic in `v1`. The model may prepare the change, but a human maintainer decides whether the workflow, skill, or documentation is ready to merge or publish.

### Deterministic Feedback Surfaces

| Primitive | Authoritative paths | ADLC role |
| --- | --- | --- |
| `gtest` | `gtest/src/lib.rs`, `gtest/src/state/gas_tree.rs`, `gtest/src/manager/journal.rs` | Primary off-chain verifier for logic, gas, and memory assumptions |
| `gear-node-wrapper` | `utils/node-wrapper/src/lib.rs`, `gcli/tests/util.rs`, `gsdk/tests/utils/mod.rs` | Local-node orchestration primitive |
| `gcli` | `gcli/src/lib.rs`, `gcli/README.md`, `gcli/tests/smoke.rs` | CLI surface for config, deploy, send, and read flows |
| `cargo sails` | `/Users/ukintvs/Documents/projects/sails/rs/cli/src/main.rs`, `/Users/ukintvs/Documents/projects/sails/templates/program/README.md` | Program scaffolding, IDL, and client generation |
| `awesome-sails` fixtures | `/Users/ukintvs/Documents/projects/awesome-sails/tests/awesome-sails-test/app/tests/gtest.rs`, `/Users/ukintvs/Documents/projects/awesome-sails/tests/access-control-test/app/tests/gtest.rs` | Realistic example and regression corpus |

### ADLC Failure Policy

ADLC workflows must fail closed in the following cases:

- unsupported target is EVM, Solidity, or non-WASM deployment;
- required local binary such as `target/release/gear` is missing for a node-wrapper flow;
- `gcli` cannot connect to the configured endpoint;
- gas or memory constraints fail during `gtest`;
- documentation or repository references do not resolve to real files or commands.

In `v1`, those failures are not opportunities for the agent to improvise a substitute workflow. They are reasons to stop and report the blocker.

## Part II: Gear Skills Standard

### Repository Choice

This RFC standardizes skills in a separate `gear-skills` repository.

Reasons:

1. Skills need their own release and review cadence.
2. A cross-repo standard should not force `gear`, `sails`, and `awesome-sails` to share one repository lifecycle.
3. Validation, metadata quality, and trigger discipline are easier to enforce centrally.
4. External orchestrators need one obvious place to look for supported Gear/Vara workflows.

`gear`, `sails`, and `awesome-sails` remain authoritative for code and examples. `gear-skills` only packages and validates how agents consume those sources.

### Canonical Skill Package Contract

Every published Gear Skill must use this directory layout:

```text
<skill-name>/
├── SKILL.md
├── scripts/
├── references/
└── assets/
```

Normative rules:

1. The directory name must exactly match the skill metadata name.
2. `SKILL.md` must stay under 500 lines.
3. The YAML frontmatter must include `name` and `description`.
4. The `description` field must be written in the third person, stay under 1,024 characters, and include negative triggers.
5. The body of `SKILL.md` must use numbered workflows and explicit decision points, not conversational prose.
6. Relative file references in `SKILL.md` resolve from the skill directory first.
7. Agents should load only the minimum reference files needed for the current step.
8. `references/` must stay one level deep to discourage partial, context-losing reads across deep trees.
9. `assets/` contains static templates, schemas, pinned snippets, or canned configs.
10. `scripts/` contains tiny deterministic CLIs. Agents execute these scripts; they do not rewrite them as part of ordinary task flow.

Example frontmatter:

```yaml
---
name: gear-test-sails-program
description: "Tests Sails-based Gear WASM programs with gtest and local build steps. Do not use this skill for EVM, Solidity, or non-Sails program workflows."
---
```

### Progressive Disclosure Rules

The skill system is designed to preserve context budget:

1. Load `SKILL.md` first.
2. Load only the referenced `scripts/`, `references/`, or `assets/` entries needed for the current step.
3. Prefer executing a validated script over re-explaining or reimplementing a brittle procedure.
4. Do not bulk-read repository documentation unless the skill explicitly requires it.

This is the primary mechanism that keeps LLM agents from exhausting context on protocol material they do not yet need.

### Public Interfaces Added by This RFC

`v1` introduces repository-level interfaces, not crate-level interfaces:

- the `gear-skills` repository contract;
- the `SKILL.md` metadata and layout contract;
- the validation pipeline for approving skills; and
- the starter catalog and ownership model.

`v1` does **not** add or modify APIs in `gtest`, `gcli`, `gear-node-wrapper`, `sails-rs`, or `awesome-sails`.

## Starter Skill Catalog

The first wave is intentionally Sails-first because that is the shortest path to successful end-to-end agent usage.

| Skill | Purpose | Authoritative sources | Example deterministic scripts | Required negative triggers |
| --- | --- | --- | --- | --- |
| `sails-new-program` | Create a new Sails program or workspace | `/Users/ukintvs/Documents/projects/sails/rs/cli/src/main.rs`, `/Users/ukintvs/Documents/projects/sails/templates/program/README.md` | `check_toolchain.sh`, `create_sails_program.sh` | Not for plain `gstd`-only apps, EVM, or Solidity |
| `gear-test-sails-program` | Build and test a Sails program with `gtest` | `gtest/src/lib.rs`, `/Users/ukintvs/Documents/projects/sails/templates/program/tests/gtest.rs`, `/Users/ukintvs/Documents/projects/awesome-sails/tests/awesome-sails-test/app/tests/gtest.rs` | `build_wasm.sh`, `run_gtest.sh` | Not for live-network-only validation or non-Sails programs |
| `gear-run-local-node` | Start and manage a local Gear/Vara node | `utils/node-wrapper/src/lib.rs`, `gcli/tests/util.rs`, `gsdk/tests/utils/mod.rs`, `node/README.md` | `spawn_dev_node.sh`, `wait_for_ws.sh`, `stop_dev_node.sh` | Not for remote RPC providers or Ethereum nodes |
| `gear-deploy-program` | Deploy a compiled program to a local or supported Gear node | `gcli/src/lib.rs`, `gcli/README.md` | `configure_gcli.sh`, `deploy_wasm.sh` | Not for Solidity/Vara.eth deployments |
| `gear-query-program-state` | Read program state or exercise post-deploy flows | `gcli/src/lib.rs`, `/Users/ukintvs/Documents/projects/sails/rs/cli/src/main.rs` | `query_state.sh`, `send_message.sh` | Not for explorer indexing, analytics pipelines, or unsupported chains |

## Validation Standard

Every skill merge in `gear-skills` must pass four validation layers.

### 1. Discovery validation

Goal: prove that the skill triggers when it should and stays dormant when it should not.

Minimum checks:

- a fresh model session selects the skill for a matching Gear/Vara/Sails task;
- the same model does not select the skill for EVM, Solidity, or unrelated Substrate tasks;
- the skill description is specific enough to avoid false positives.

### 2. Logic validation

Goal: prove that each step resolves to a real command, file, or script.

Minimum checks:

- every repository path named in the skill exists;
- every command exists in the current tool surface;
- every script runs or dry-runs successfully in the intended environment; and
- the workflow does not require unexplained repository spelunking.

### 3. Edge-case validation

Goal: prove that the skill fails safely and informatively.

Required edge cases for the first-wave catalog:

- failing `gtest`;
- gas limit or allocation failures surfaced locally;
- missing `target/release/gear` for node-wrapper flows;
- broken `gcli` config, endpoint, or wallet setup;
- unsupported target types such as EVM, Solidity, or non-WASM outputs.

### 4. Pilot validation

Goal: prove that the catalog works as a real end-to-end workflow instead of five isolated micro-skills.

The initial pilot uses `gear`, `sails`, and `awesome-sails` as the validation corpus.

Required pilot flow:

1. Create or select a Sails program using `cargo sails`.
2. Build and run the local `gtest` suite.
3. Start a local node using `gear-node-wrapper` or `gear --dev`.
4. Configure `gcli` against that endpoint and initialize a local wallet.
5. Deploy the compiled Wasm with `gcli`.
6. Query or send a message using `gcli`.
7. Cross-check the workflow against a realistic `awesome-sails` fixture to capture any missing context.

Exit criteria:

- all five starter skills trigger correctly in a fresh model session;
- every referenced path or command resolves cleanly;
- the pilot flow completes without ad hoc repository exploration; and
- unsupported targets fail closed with explicit negative-trigger behavior.

## Ownership and Governance

The validation model should be strict because SkillsBench-style benchmark evidence favors expert-authored skills over self-generated ones.

That is the core governance assumption behind this RFC: maintainers and expert contributors author the procedural knowledge, while agents consume it. `v1` does not rely on agents inventing their own durable Gear Skills.

Ownership model for `v1`:

- Gear maintainers own skills whose primary surfaces are `gtest`, `gcli`, `gear-node-wrapper`, or local node flows.
- Sails maintainers own skills whose primary surfaces are `cargo sails`, templates, IDL generation, and generated clients.
- Community contributors may propose new skills, but merge requires approval from the owning maintainers.
- `awesome-sails` serves as a fixture and regression source, not as the normative home of the standard itself.

This keeps authority close to the repositories that actually define behavior while preserving a single public catalog for agent tooling.

## Out of Scope

This RFC does not attempt to:

- standardize EVM or Solidity deployment workflows;
- standardize Vara.eth contract workflows;
- let agents publish skills without maintainer review;
- replace human design or release approval; or
- change public crate APIs in `gear`, `sails`, or `awesome-sails`.

## Immediate Follow-Up

If this RFC is accepted, the next implementation step is to create the `gear-skills` repository and land the first-wave catalog with validation fixtures drawn from the current `gear`, `sails`, and `awesome-sails` repositories.
