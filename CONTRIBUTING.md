# 📄 CONTRIBUTING.md

## 1. Purpose

This document defines the development workflow, naming conventions, and repository rules.

All contributors are expected to follow these rules to ensure consistency, clarity, and fast iteration.

---

## 2. Workflow

All work MUST follow this flow:

Issue → Branch → Pull Request → Review → Merge (squash via merge queue)

---

## 3. Issues

### 3.1 Requirement

- Every task MUST be tracked via a GitHub issue
- No work MUST be performed without an associated issue

### 3.2 Issue Titles

- Issue titles MUST describe the problem or goal
- Issue titles MUST NOT use commit-style prefixes (e.g. `feat(...)`, `fix(...)`)

Examples:

- Gas calculation overflows on large inputs
- Add withdrawal flow for vara.eth bridge

### 3.3 Issue Labels

Each issue MUST include:

- `type:*`
- `scope:*`

The following labels SHOULD be added when applicable:

- `priority:*`
- `size:*`

### 3.4 Issue Ownership

An issue is considered **in progress** when it is assigned.

Unassigned issues are considered open for anyone to take.

---

## 4. Branch Naming

Branches MUST follow the format:

<initials-or-nickname>/<short-description>

Examples:

- pp/impl-web
- ab/fix-gas-overflow
- jd/add-withdrawal-flow

---

## 5. Pull Requests

### 5.1 Draft PR

A Pull Request MUST be created as **Draft** if:

- work is incomplete
- the PR is a proof-of-concept (PoC)
- early feedback is required

### 5.2 Ready PR

A Pull Request MUST be marked as **Ready for review** when:

- implementation is complete
- the PR is ready for full review

### 5.3 PR States

- Draft — work in progress or PoC (not ready for review)
- Ready for review — ready for review
- Changes requested — issues identified, updates required
- Approved — ready to merge (subject to CI)

---

## 6. Pull Request Naming

All Pull Request titles MUST follow Conventional Commits:

<type>(<scope>/<optional>): <description>
<type>!: <description>

### 6.1 Allowed Types

- feat
- fix
- refactor
- docs
- test
- chore

### 6.2 Allowed Scopes

- gear
- vara
- vara.eth
- programs

### 6.3 Breaking Changes

Breaking changes MUST be indicated using `!`:

refactor(programs)!: remove deprecated API

---

## 7. Labels

### 7.1 Issue Labels

Allowed labels:

- type: *
- scope: *
- priority: critical | important | normal | backlog
- size: S | M | L | XL
- ai-friendly (optional)
- ai-generated (optional)

#### Priority Semantics

- critical — must be addressed immediately (production, security, or consensus impact)
- important — should be addressed soon; blocks meaningful progress or upcoming release
- normal — standard planned work
- backlog — low priority; no immediate action required

#### Size Semantics

- S — small task (hours)
- M — medium task (1–2 days)
- L — large task (several days)
- XL — very large task; SHOULD be split if possible

---

### 7.2 Pull Request Labels

Allowed labels:

- type: *
- scope: *
- ci: *
- ai-friendly (optional)
- ai-generated (optional)
- pr: do-not-merge

### 7.3 Restrictions

- `priority:*` MUST NOT be used on Pull Requests
- `size:*` MUST NOT be used on Pull Requests

---

## 8. CI Labels

CI labels are used ONLY on Pull Requests.

Available labels:

- ci: docker
- ci: windows
- ci: macos
- ci: linux-aarch64
- ci: release
- ci: production
- ci: full

### 8.1 Semantics

- ci: release → runs `cargo --release`
- ci: production → runs `cargo --profile production`
- ci: full → runs full CI matrix (all platforms and profiles)

### 8.2 Behavior

CI labels are **reactive** and may be modified automatically by CI:

- labels MAY be added or removed based on PR title or other labels
- `type` and `scope` MAY be derived from PR title
- `ci: full` overrides all other `ci:*` labels
- redundant labels MAY be removed automatically

### 8.3 Usage Rules

- No more than two CI labels SHOULD be used per PR
- If more coverage is required, `ci: full` SHOULD be used

---

## 9. Merge Strategy

- All Pull Requests MUST go through **merge queue**
- All merges are performed via **squash merge**
- The Pull Request title MUST be used as the final commit message

---

## 10. Review Process

### 10.1 Addressing Feedback

If the author agrees with a review comment:

- the author MUST push the required changes
- the author MUST resolve the conversation

If the author disagrees:

- the author MUST provide reasoning
- agreement MUST be reached with the reviewer
- the reviewer MUST resolve the conversation if they withdraw the comment

### 10.2 Changes After Approval

If changes are pushed after approval:

- the author MUST notify the reviewer if the changes are non-trivial
- the author MUST request re-review if behavior or logic has changed
- silent changes after approval MUST be avoided

For breaking or risky changes, re-review is REQUIRED.

---

## 11. Special Labels

### pr: do-not-merge

The Pull Request MUST NOT be merged while this label is present.

This label is enforced via CI and blocks merge queue.

---

### ai-generated

Indicates that the issue or Pull Request was created entirely by an AI agent, without direct human authorship.

This label MUST be applied when:

- the issue was opened by an AI agent autonomously
- the PR was authored and submitted by an AI agent end-to-end

Human review of `ai-generated` issues and Pull Requests is REQUIRED before merging.

---

### ai-friendly

Indicates that the task is suitable for AI-assisted work.

This label MAY be applied to both **issues** and **Pull Requests**.

On an issue, it signals that contributors are encouraged to point their AI agents at it — the task is scoped well enough for autonomous implementation.

This includes tasks that:

- can be fully implemented by AI
- can be reliably reviewed by AI
- have low architectural or consensus risk

This label is used to improve development speed, not to reduce quality standards.

---

## 12. General Rules

- A Pull Request MUST represent a single logical change
- Large Pull Requests SHOULD be avoided
- Tests SHOULD be added or updated when modifying logic
- Documentation SHOULD be updated when necessary
- Draft PRs SHOULD be used early to share progress and gather feedback

---

## 13. Principles

Issue = problem or goal
Pull Request = implementation

Labels = classification
CI labels = execution control

Pull Request title = source of truth for release notes

---

## 14. Licensing

All `.rs` source files MUST include a license header as the first two lines:

```rust
// Copyright (C) Gear Technologies Inc.
// SPDX-License-Identifier: GPL-3.0-or-later WITH Classpath-exception-2.0
```

A blank line MUST follow the header before any code or module-level comments.

Files derived from third-party sources MUST preserve the original copyright line and MUST NOT include the Gear copyright line:

```rust
// Copyright (C) 2017-2024 Parity Technologies.
// SPDX-License-Identifier: GPL-3.0-or-later WITH Classpath-exception-2.0
```

The `LICENSE` file in the repository root is the authoritative license text.

---

This process is intentionally minimal and designed for a small, fast-moving team.
