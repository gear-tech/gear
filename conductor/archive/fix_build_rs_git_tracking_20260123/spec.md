# Specification: Improve Git Tracking in ethexe-cli build.rs

## Overview
The current `build.rs` in `ethexe/cli` uses `cargo:rerun-if-changed=../../.git/refs`, which is unreliable for detecting commit or branch changes because directory timestamps may not update. This track replaces it with a robust mechanism that tracks `.git/HEAD` and the specific ref file it points to.

## Functional Requirements
- Build script must correctly detect commit changes and branch switches.
- Replace directory-based `rerun-if-changed` with explicit file-based tracking.

## Acceptance Criteria
- `ethexe/cli/build.rs` is updated to read `.git/HEAD`.
- If `.git/HEAD` contains a ref, the build script instructs Cargo to watch that specific ref file.
- `cargo:rerun-if-changed=../../.git/HEAD` is maintained.
