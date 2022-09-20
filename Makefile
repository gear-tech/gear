.PHONY: fmt clippy test pre-commit

pre-commit: fmt clippy test

fmt:
	@ cargo +nightly fmt --all

clippy:
	@ SKIP_WASM_BUILD=1 cargo clippy --workspace --all-targets \
		--all-features -- --no-deps -D warnings

test: # This requires run of `./scripts/donwload-gear.sh res`
	@ cargo test --all --all-features
