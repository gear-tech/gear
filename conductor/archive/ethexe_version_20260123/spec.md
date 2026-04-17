# Specification: ethexe-cli Version Argument

## Overview
Add support for a `--version` flag to the `ethexe-cli` to provide users with precise build information, including the semantic version and the short commit hash of the source code.

## Functional Requirements
- The `ethexe-cli` executable must accept `--version` and `-V` flags.
- The output must follow the format: `<semver>-<short-hash>`.
- Example output: `0.1.10-a1b2c3d`.
- The command must exit successfully after printing the version.

## Non-Functional Requirements
- Performance: The version check should be instantaneous.
- Maintainability: Version information should be derived from the crate's `Cargo.toml` or workspace metadata.

## Acceptance Criteria
- Running `ethexe --version` prints the correct version and hash.
- Running `ethexe -V` prints the correct version and hash.
- The short hash corresponds to the current HEAD at the time of build.
