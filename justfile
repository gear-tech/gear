[private]
default:
    @just --list --unsorted

# Remove untracked files and build caches
[confirm('Remove all untracked files and build caches? (y/n)')]
clean:
    git clean -fdx

# Run pre-commit checks
[group('checks')]
pre-commit: fmt typos clippy test check-runtime-imports

# Format code via `rustfmt`
[group('checks')]
fmt:
    # Running `rustfmt` on the workspace
    cargo fmt --all

# Check code formatting via `rustfmt`
[group('checks')]
fmt-check:
    # Running `rustfmt` on the workspace in check mode
    cargo fmt --all --check

# Check code for typos via `typos-cli`
[group('checks')]
typos: (ensure-binary "typos" "cargo install typos-cli")
    # Checking the repository for typos
    typos

# Check code with `cargo check`
[group('checks')]
check:
    # Check the workspace
    @ __GEAR_WASM_BUILDER_NO_BUILD=1 SKIP_WASM_BUILD=1 \
        cargo check --workspace
    # Check crates that use `cfg(fuzz)`
    @ RUSTFLAGS="--cfg fuzz" \
        cargo check "$@" \
        -p gear-common \
        -p vara-runtime \
        -p runtime-fuzzer \
        -p runtime-fuzzer-fuzz


# Check all code with Clippy
[group('checks')]
clippy: clippy-gear clippy-examples-wasm

# Check all native code with Clippy
[group('checks')]
clippy-gear:
    # Check the whole workspace
    @ __GEAR_WASM_BUILDER_NO_BUILD=1 SKIP_WASM_BUILD=1 \
      cargo clippy --workspace --all-targets --all-features -- --no-deps -D warnings

# Check examples with Clippy
[group('checks')]
clippy-examples: clippy-examples-native clippy-examples-wasm

[private]
clippy-examples-native:
    # Check native parts of examples
    @ __GEAR_WASM_BUILDER_NO_BUILD=1 SKIP_WASM_BUILD=1 \
      cargo clippy -p 'demo-*' -p 'test-syscalls' \
      --all-targets --all-features -- --no-deps -D warnings

[private]
clippy-examples-wasm:
    # Check WebAssembly parts of examples
    @ cargo metadata --no-deps --format-version=1 \
      | jq -r '.packages.[] | select(.manifest_path | contains("gear/examples")) | select(.dependencies.[].name == "gear-wasm-builder") | "-p=" + .name' \
      | __GEAR_WASM_BUILDER_NO_BUILD=1 SKIP_WASM_BUILD=1 \
        xargs sh -c 'cargo clippy "$@" --no-default-features --target=wasm32v1-none -- -D warnings'

# Run tests
[group('checks')]
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

# Check runtime imports
[group('checks')]
check-runtime-imports:
    # Checking runtime imports
    cargo build -p wasm-proc
    cargo build -p vara-runtime
    ./target/debug/wasm-proc \
        --check-runtime-imports \
        target/debug/wbuild/vara-runtime/vara_runtime.wasm


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
