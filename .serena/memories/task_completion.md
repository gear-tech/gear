# When Finishing a Task
- Run formatting (`make fmt`) and ensure clippy is clean for touched crates (`make clippy` or narrower gear.sh clippy commands).
- Execute relevant tests for affected areas; default is `make test` (may be heavy) or scope to pallets/runtime/client examples as appropriate (e.g., `make test-pallet`, `make test-gsdk`).
- For comprehensive verification, `make pre-commit` (fmt + typos + clippy + test + runtime-import checks) when time allows.
- If changing node binaries/runtime, consider rebuild: `make node` or `make gear` and rerun smoke tests as needed.
- Document notable changes/commands run in PR description; use `[skip-ci]` only if intentionally bypassing CI.