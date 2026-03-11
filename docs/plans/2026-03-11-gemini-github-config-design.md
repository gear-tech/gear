# Gemini Code Assist GitHub Configuration Design

## Status

Approved design for repository-level Gemini Code Assist customization in GitHub.

## Goal

Add a repository-local Gemini configuration and review style guide that makes Gemini useful for `gear` maintainers without turning it into a noisy generic reviewer.

The design targets the current GitHub integration documented by Google for repositories that customize Gemini behavior through:

1. `.gemini/config.yaml`
2. `.gemini/styleguide.md`

## Scope

In scope:

1. Repository-level Gemini behavior for GitHub pull requests.
2. Review posture for the whole workspace, not just `ethexe`.
3. A balanced review policy: correctness first, then maintainability when it materially affects safety or review quality.
4. Ignoring generated and machine-produced outputs for direct review comments.
5. A rollout and validation model for checking whether the configuration improves review signal.

Out of scope:

1. IDE/editor Gemini behavior.
2. Organization-level Google Cloud console settings.
3. Changes to CI, branch protection, or required GitHub checks.
4. Per-team or per-directory ownership rules.
5. Replacing human review with Gemini review.

## Confirmed Decisions

1. Reviewer posture: balanced.
2. Repository emphasis: whole workspace equally.
3. Generated artifacts: ignored for direct review comments by default.
4. Automation model: automatic PR summary and automatic review on PR open remain enabled.
5. Noise control: use repository config only for high-level automation and path ignores; put repository-specific review policy in the style guide.

## Repository Context

The repository is a large mixed-language workspace with:

1. A Rust 2024 workspace in `/Users/ukintvs/Documents/projects/gear/Cargo.toml`.
2. Solidity contracts and Ethereum-facing artifacts under `/Users/ukintvs/Documents/projects/gear/ethexe/contracts` and `/Users/ukintvs/Documents/projects/gear/ethexe/ethereum/abi`.
3. Centralized Rust build, check, format, and test entrypoints through `/Users/ukintvs/Documents/projects/gear/scripts/gear.sh`.
4. Strict CI enforcement in `/Users/ukintvs/Documents/projects/gear/.github/workflows/check.yml`.
5. Existing design documentation patterns in `/Users/ukintvs/Documents/projects/gear/docs/plans`.

Observed repository conventions relevant to Gemini guidance:

1. Formatting is already enforced through `rustfmt`, `forge fmt`, and existing CI checks.
2. Review-critical changes often require updates to generated artifacts, ABI files, scripts, or CI enforcement.
3. Recent merged PRs and issues are concentrated around correctness, protocol invariants, upgrade safety, batching/limits, and missing verification rather than cosmetic style.
4. The repository uses generated or derived files that should not become primary review targets.

## External Constraints From Gemini GitHub Customization

According to Google's Gemini Code Assist GitHub customization documentation:

1. `.gemini/config.yaml` controls automation and review settings such as:
   1. `code_review.disable`
   2. `code_review.comment_severity_threshold`
   3. `code_review.max_review_comments`
   4. `code_review.pull_request_opened.help`
   5. `code_review.pull_request_opened.summary`
   6. `code_review.pull_request_opened.code_review`
   7. `code_review.pull_request_opened.include_drafts`
   8. `ignore_patterns`
2. `.gemini/styleguide.md` has no fixed schema and acts as natural-language instructions for how Gemini should review the repository.
3. Repository `config.yaml` overrides console configuration for overlapping settings.
4. Repository `styleguide.md` is combined with any higher-level style guide rather than replacing it.

This means the repository should keep `config.yaml` small and deterministic while using `styleguide.md` for review policy.

## Proposed Files

Add two files at the repository root:

1. `/Users/ukintvs/Documents/projects/gear/.gemini/config.yaml`
2. `/Users/ukintvs/Documents/projects/gear/.gemini/styleguide.md`

No additional Gemini files are required for `v1`.

## Design Overview

### 1. Configuration Role

`config.yaml` should only answer three questions:

1. When should Gemini speak automatically?
2. How much noise is acceptable?
3. Which paths should Gemini ignore entirely?

It should not try to encode the repository's full review culture.

### 2. Style Guide Role

`styleguide.md` should encode the repository's actual reviewer priorities:

1. correctness
2. safety-critical changes
3. verification gaps
4. maintainability only when it affects review quality or future regressions

This keeps the behavior flexible without turning configuration into an unreadable policy document.

## Proposed `config.yaml`

The repository configuration should stay close to Gemini defaults while reducing obvious low-value output.

### PR-open behavior

1. Keep automatic PR summaries enabled.
2. Keep automatic code review on PR open enabled.
3. Disable help comments on PR open.
4. Exclude draft pull requests from automatic review.

Reasoning:

1. The repository benefits from passive review coverage on real PRs.
2. Draft PRs are likely to churn and would create noise before the author is ready.
3. Help comments are low-value compared with summaries and actual review findings.

### Comment quality controls

1. Set `comment_severity_threshold` to `MEDIUM`.
2. Set `max_review_comments` to a small cap, recommended `6`.

Reasoning:

1. A balanced posture should still catch correctness and maintainability issues that matter.
2. A `MEDIUM` floor aligns with the repository's preference for actionable findings over stylistic nits.
3. A low comment cap prevents large PRs from being overwhelmed by fragmented review output.

### Ignore patterns

Ignore direct review on clearly non-authoritative or machine-produced paths. The initial ignore set should include:

1. `target/**`
2. `.worktrees/**`
3. `ethexe/contracts/out/**`
4. `ethexe/ethereum/abi/*.json`

Potential additions if noise shows up later:

1. transient local tooling output directories
2. vendored build artifacts or generated lockstep mirrors

Important semantic rule for the style guide:

Gemini should ignore generated outputs as direct review targets, but it should still flag source-of-truth changes that appear to require regenerated artifacts and those artifacts are missing or stale.

## Proposed `styleguide.md`

The style guide should be repository-specific reviewer policy, not a contributor tutorial.

### Core priorities

Gemini should prioritize findings in this order:

1. Correctness and protocol behavior.
2. Safety-critical changes.
3. Verification gaps.
4. Maintainability with concrete review impact.

### Correctness and protocol behavior

Gemini should look first for:

1. logic errors
2. broken edge cases
3. concurrency or race risks
4. invalid state transitions
5. incorrect event handling
6. ordering assumptions that can break under real execution
7. incorrect API or tool usage
8. accidental behavior changes in runtime, protocol, RPC, batching, or queue logic

This priority matches recent repository review activity, especially in `ethexe`-related work.

### Safety-critical changes

Gemini should treat the following as high-attention areas:

1. contract upgrades and migrations
2. access control
3. consensus-sensitive logic
4. validator or batch-processing limits
5. ABI compatibility
6. generated artifact drift
7. changes that affect externally visible behavior without clear verification

For Solidity and Ethereum-adjacent changes, the style guide should explicitly call out:

1. upgrade safety
2. storage compatibility
3. authorization regressions
4. deployment or upgrade script mismatches
5. source-to-ABI consistency expectations

### Verification gaps

Gemini should strongly prefer comments about missing verification over comments about code style.

Examples:

1. behavior changes without tests
2. new invariants without CI enforcement
3. source changes that appear to require regenerated ABI or artifact updates
4. deployment or workflow changes without corresponding docs or scripts updates when those are the source of truth
5. configuration or manifest changes without checks for toolchain or workspace consistency

### Maintainability with concrete impact

Gemini may comment on maintainability only when the issue has practical review impact, such as:

1. code that obscures correctness
2. duplication that makes bugs likely to recur
3. missing structure around risky branches or invariants
4. changes that make future debugging materially harder

It should avoid taste-based or purely aesthetic comments.

## Explicit Anti-Noise Rules

The style guide should tell Gemini not to:

1. comment on formatting already enforced by `rustfmt`, `forge fmt`, or repository linting
2. review generated JSON or ABI files directly
3. suggest broad refactors without a clear correctness, verification, or maintenance benefit
4. flood a PR with many low-signal comments when one higher-signal comment is enough
5. focus on naming, wording, or docs style unless the change makes behavior misleading or incomplete
6. treat speculative risks as findings without tying them to changed code

## Repository-Specific Review Rules

The style guide should include concrete repository cues:

1. Prefer reviewing source-of-truth files over generated artifacts.
2. If `/Users/ukintvs/Documents/projects/gear/ethexe/contracts/src` changes, consider whether tests, ABI files, scripts, or relevant README instructions must also change.
3. If CI or workflow files change, focus on whether enforcement was weakened, coverage was reduced, or important checks can now be bypassed.
4. If `/Users/ukintvs/Documents/projects/gear/Cargo.toml`, `rust-toolchain.toml`, or workspace patches change, focus on cross-workspace effects, toolchain alignment, and version pinning risk.
5. If protocol behavior changes, expect deterministic verification paths and concrete evidence, not reasoning alone.
6. Prefer comments on missing tests or missing regeneration steps over comments on superficial code organization.

## Behavior On Generated Files

Generated files are intentionally ignored for direct review comments, but Gemini should still reason about them indirectly.

Allowed behavior:

1. "This source change appears to require updating generated ABI artifacts."
2. "This contract change may need deployment script or artifact regeneration coverage."
3. "This source-of-truth change is not reflected in committed generated outputs."

Disallowed behavior:

1. reviewing JSON formatting in generated ABI files
2. nitpicking generated code shape
3. treating generated files as the primary location for review findings

## Alternatives Considered

### Option 1: Balanced reviewer, minimal automation, strong style guide

Selected.

Pros:

1. Good signal-to-noise ratio.
2. Keeps passive review coverage.
3. Adapts to the repository's actual review culture.
4. Easy to evolve by editing one natural-language policy file.

Cons:

1. Relies on a well-written style guide.
2. Some path-specific nuance remains implicit rather than machine-validated.

### Option 2: Stricter automation with auto review disabled on PR open

Rejected.

Reason:

1. Lowest noise, but removes passive coverage and depends on contributors explicitly invoking Gemini.

### Option 3: Broader review assistant with lower thresholds

Rejected.

Reason:

1. Would produce more style, docs, and ergonomics comments than the repository currently appears to want.

## Rollout Plan

### Phase 1: Initial repository files

1. Create `.gemini/config.yaml`.
2. Create `.gemini/styleguide.md`.
3. Keep config conservative and stable.

### Phase 2: Dry validation against representative PR classes

Evaluate the guidance against recent or typical PR shapes:

1. contract or upgrade-safety changes
2. `ethexe` protocol or batching changes
3. CI enforcement changes
4. docs-only changes
5. generated-artifact-heavy changes

### Phase 3: Tune only if clear noise patterns appear

Prefer tuning the style guide first. Change `config.yaml` only if:

1. severity filtering is clearly wrong
2. review volume is too high
3. ignored-path behavior needs correction

## Acceptance Criteria

The rollout is successful if:

1. Gemini comments mostly target real defects, risky omissions, or missing verification.
2. Gemini avoids formatting comments and generated-file nitpicks.
3. Gemini does not spam large PRs with fragmented review output.
4. Automatic PR summaries remain useful and non-invasive.
5. The configuration works across the whole workspace without biasing toward only one subsystem.

The rollout is unsuccessful if:

1. Gemini frequently comments on already-enforced formatting or lint issues.
2. Gemini treats generated artifacts as primary review subjects.
3. Gemini misses obvious source-to-generated drift patterns because the ignore rules are too aggressive.
4. Review volume becomes distracting relative to the value of the findings.

## Risks and Trade-offs

1. Ignoring generated outputs reduces noise, but can hide useful context if the style guide does not explicitly preserve source-to-generated consistency checks.
2. A natural-language style guide is flexible, but behavior will never be perfectly deterministic.
3. A whole-workspace policy may be slightly less optimized for `ethexe` than a subsystem-specific guide, but it avoids teaching Gemini the wrong bias for the rest of the repository.
4. Keeping automatic review on PR open preserves coverage, but some residual noise is unavoidable even with a medium severity threshold.

## Implementation Notes For The Follow-Up Plan

The implementation plan should:

1. create `.gemini/config.yaml` with the selected settings
2. create `.gemini/styleguide.md` with repository-specific reviewer policy
3. validate path choices against the current repository layout
4. verify that the files are readable, minimal, and aligned with the Google-documented schema and behavior
5. document any assumptions that remain intentionally heuristic

## References

1. Google Gemini Code Assist GitHub customization docs: `https://developers.google.com/gemini-code-assist/docs/customize-gemini-behavior-github`
2. Repository root README: `/Users/ukintvs/Documents/projects/gear/README.md`
3. Development guidance: `/Users/ukintvs/Documents/projects/gear/DEVELOPMENT.md`
4. CI checks: `/Users/ukintvs/Documents/projects/gear/.github/workflows/check.yml`
5. Central repository command entrypoint: `/Users/ukintvs/Documents/projects/gear/scripts/gear.sh`
