# Style and Conventions
- Language: Rust 2024 edition across workspace; follow standard Rust patterns for Substrate pallets/runtime and async actor-style contracts.
- Formatting: rustfmt with `imports_granularity = "Crate"` and doc-comment formatting enabled; use `cargo fmt` via `make fmt` or `./scripts/gear.sh format gear`. Editorconfig enforces LF and final newline.
- Lints: prefer clean clippy runs (`make clippy` / `./scripts/gear.sh clippy gear`/`examples`); tests use cargo-nextest where configured.
- CI skip token is `[skip-ci]` (not `[skip ci]`).
- Keep comments concise; align with existing module organization (pallets/runtime/SDK crates) and Substrate naming conventions.