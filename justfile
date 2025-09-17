[private]
default:
    @just --list --unsorted

# Remove untracked files and build caches
[confirm('Remove all untracked files and build caches? (y/n)')]
clean:
    git clean -fdx

# Format code via `rustfmt`
fmt:
    cargo fmt --all

# Check code formatting via `rustfmt`
fmt-check:
    cargo fmt --all --check

# Check code for typos via `typos-cli`
typos: (ensure-binary "typos" "cargo install typos-cli")
    typos

# Check code with Clippy
clippy:
    # Check the whole workspace
    @ __GEAR_WASM_BUILDER_NO_BUILD="1" SKIP_WASM_BUILD="1" \
      cargo clippy --workspace --all-targets --all-features -- --no-deps -D warnings

    # Check WebAssembly crates
    @ cargo metadata --no-deps --format-version=1 \
      | jq -r '.packages.[] | select(.manifest_path | contains("gear/examples")) | select(.dependencies.[].name == "gear-wasm-builder") | "-p=" + .name' \
      | __GEAR_WASM_BUILDER_NO_BUILD=1 SKIP_WASM_BUILD=1 \
        xargs sh -c 'cargo clippy "$@" --no-default-features --target=wasm32v1-none -- -D warnings'

# Run tests
test: (ensure-cargo "hack") (ensure-cargo "nextest")
    # Running workspace tests
    @ cargo nextest run \
        --workspace \
        --no-fail-fast \
        --exclude gclient \
        --exclude gcli \
        --exclude gsdk \
        --exclude gear-authorship \
        --exclude pallet-gear-staking-rewards \
        --exclude gear-wasm-gen \
        --exclude demo-stack-allocations \
        --exclude gring \
        --exclude runtime-fuzzer \
        --exclude runtime-fuzzer-fuzz

[private]
ensure-binary binary hint: (
    ensure
    ("command -v " + binary)
    ("`" + binary + "` program")
    hint
)

[private]
ensure-cargo subcommand: (
    ensure
    ("cargo --list | awk '{ print $1 }' | grep '^" + subcommand + "$'")
    ("`cargo " + subcommand + "` subcommand")
    ("cargo install cargo-" + subcommand)
)

[private]
ensure condition what hint:
    @if ! ({{ condition }}) >/dev/null; then \
        echo 'You need {{ what }} to run this script.' >&2 ;\
        echo >&2 ;\
        echo 'To install it, run following command:' >&2 ;\
        echo '> {{ hint }}' >&2 ;\
        echo >&2 ;\
        exit 1 ;\
    fi
