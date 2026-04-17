# Suggested Commands
- Show help: `./scripts/gear.sh --help` (per-command usage) or review `Makefile` targets.
- Format: `make fmt` (rustfmt whole workspace) or check-only `make fmt-check`.
- Lint: `make clippy` (workspace + examples) or `make clippy-release` for release mode.
- Test: `make test` (workspace tests minus some heavy crates) or targeted `make test-pallet`, `make test-gsdk`, `make test-gcli`; doc tests via `./scripts/gear.sh test docs`.
- Pre-flight: `make pre-commit` runs fmt + typos + clippy + test + runtime import checks; heavier but good for full validation.
- Build: `make gear` (workspace), `make node` (node binary), `make gear-release`/`node-release` for release builds.
- Run node: `make run-node` or `make run-dev-node` (sets debug logs).
- Init tooling: `./scripts/gear.sh init cargo` (installs cargo-hack and cargo-nextest), `./scripts/gear.sh init wasm` for WASM toolchain setup.
- Clean: `make clean` (cargo clean + git clean -fdx) — beware it removes untracked files.