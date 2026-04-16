# Gemini GitHub Config Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Add repository-local Gemini Code Assist GitHub configuration and a repo-specific review style guide that preserve automatic PR coverage while steering review output toward correctness, safety, and verification.

**Architecture:** The implementation adds two files under `.gemini/`. `.gemini/config.yaml` controls automation, comment thresholds, and ignored generated paths; `.gemini/styleguide.md` encodes repository-specific reviewer priorities and anti-noise rules for the whole workspace. Validation is content- and path-based because Gemini's GitHub behavior is configured declaratively rather than through executable code in this repository.

**Tech Stack:** GitHub Gemini Code Assist, YAML, Markdown, git, `rg`, `sed`

**Execution Note:** Run this plan in a dedicated worktree created with `@using-git-worktrees` on branch `codex/gemini-github-config`. Do not execute it in a dirty checkout.

---

### Task 1: Add `.gemini/config.yaml`

**Files:**
- Create: `.gemini/config.yaml`
- Reference: `docs/plans/2026-03-11-gemini-github-config-design.md`

**Step 1: Verify the config file does not already exist**

Run:

```bash
test -f .gemini/config.yaml
```

Expected: FAIL with exit code `1` because the file does not exist yet.

**Step 2: Create the minimal repository config**

Write `.gemini/config.yaml` with exactly this content:

```yaml
have_fun: false
code_review:
  comment_severity_threshold: MEDIUM
  max_review_comments: 6
  pull_request_opened:
    help: false
    summary: true
    code_review: true
    include_drafts: false
ignore_patterns:
  - target/**
  - .worktrees/**
  - ethexe/contracts/out/**
  - ethexe/ethereum/abi/*.json
```

**Step 3: Verify the selected keys are present**

Run:

```bash
rg -n "have_fun|comment_severity_threshold|max_review_comments|help:|summary:|code_review:|include_drafts|ignore_patterns" .gemini/config.yaml
```

Expected: PASS and print each configured key from `.gemini/config.yaml`.

**Step 4: Verify each ignored path matches a real repository area**

Run:

```bash
for path in target .worktrees ethexe/contracts/out; do
  test -e "$path" || exit 1
done
ls ethexe/ethereum/abi/*.json >/dev/null
```

Expected: PASS with exit code `0`, proving the ignore patterns target real generated or local artifact paths.

**Step 5: Check diff hygiene**

Run:

```bash
git diff --check -- .gemini/config.yaml
```

Expected: PASS with no output.

**Step 6: Commit**

```bash
git add .gemini/config.yaml
git commit -m "chore: add Gemini GitHub config"
```

### Task 2: Add `.gemini/styleguide.md`

**Files:**
- Create: `.gemini/styleguide.md`
- Reference: `docs/plans/2026-03-11-gemini-github-config-design.md`

**Step 1: Verify the style guide does not already exist**

Run:

```bash
test -f .gemini/styleguide.md
```

Expected: FAIL with exit code `1` because the file does not exist yet.

**Step 2: Create the repository review style guide**

Write `.gemini/styleguide.md` with exactly this content:

```md
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
```

**Step 3: Verify the main sections exist**

Run:

```bash
rg -n "^## " .gemini/styleguide.md
```

Expected: PASS and print the `Purpose`, `Review Priorities`, `Correctness And Protocol Behavior`, `Safety-Critical Changes`, `Verification Expectations`, `Anti-Noise Rules`, `Repository-Specific Cues`, and `Generated Files` sections.

**Step 4: Verify the repo-specific cues mention the required source-of-truth paths**

Run:

```bash
rg -n "ethexe/contracts/src/|\\.github/workflows/|Cargo.toml|rust-toolchain.toml|generated JSON or ABI files|rustfmt|forge fmt" .gemini/styleguide.md
```

Expected: PASS and print lines covering generated-file policy, formatting anti-noise rules, and repository-specific source-of-truth cues.

**Step 5: Check diff hygiene**

Run:

```bash
git diff --check -- .gemini/styleguide.md
```

Expected: PASS with no output.

**Step 6: Commit**

```bash
git add .gemini/styleguide.md
git commit -m "docs: add Gemini review style guide"
```

### Task 3: Final Verification

**Files:**
- Verify: `.gemini/config.yaml`
- Verify: `.gemini/styleguide.md`
- Reference: `docs/plans/2026-03-11-gemini-github-config-design.md`

**Step 1: Inspect the final config and style guide together**

Run:

```bash
sed -n '1,200p' .gemini/config.yaml
sed -n '1,260p' .gemini/styleguide.md
```

Expected: PASS and show a small config file plus a style guide that prioritizes correctness, safety, and verification over formatting or generated-file review.

**Step 2: Re-check the exact ignored paths and generated-file policy**

Run:

```bash
for path in target .worktrees ethexe/contracts/out; do
  test -e "$path" || exit 1
done
ls ethexe/ethereum/abi/*.json >/dev/null
rg -n "generated files are not primary review targets|Do not:|Prefer reviewing source-of-truth files" .gemini/styleguide.md
```

Expected: PASS with exit code `0`, confirming the ignored paths still exist and the style guide explicitly protects source-of-truth review focus.

**Step 3: Verify only the intended files changed**

Run:

```bash
git status --short
```

Expected: PASS and show only the intended `.gemini/` changes, plus any unrelated pre-existing changes outside the isolated worktree if the setup step was skipped by mistake.

**Step 4: Run final diff hygiene**

Run:

```bash
git diff --check
```

Expected: PASS with no output.

**Step 5: Summarize validation evidence in the handoff**

Include in the handoff:

1. The final `.gemini/config.yaml` values.
2. The ignored path list.
3. The top review priorities from `.gemini/styleguide.md`.
4. The verification commands that passed.
