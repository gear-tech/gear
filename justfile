import 'just/lib.just'

# `ethexe`-related scripts
[group('subprojects')]
mod ethexe 'just/ethexe.just'

# Check code with Clippy
[group('checks')]
mod clippy 'just/clippy.just'

[private]
@default:
    echo 'To list available recipes:'
    echo '> just --list'
    echo
    echo 'To list available recipes from namespace:'
    echo '> just --list <namespace>'

# Remove untracked files and build caches
[group('actions')]
[confirm('Remove all untracked files and build caches? (y/n)')]
clean:
    git clean -fdx

# Run pre-commit tasks and checks
[group('checks')]
pre-commit: fmt-check typos clippy::all test check-runtime-imports

# Format code via `rustfmt`
[group('actions')]
fmt:
    cargo fmt --all

# Check formatting with `rustfmt`
[group('checks')]
fmt-check:
    cargo fmt --all --check

# Check code for typos via `typos-cli`
[group('checks')]
typos: (ensure-binary "typos" "cargo install typos-cli")
    # Checking the repository for typos
    typos

# Run tests
[group('checks')]
test: (ensure-cargo "hack") (ensure-cargo "nextest")
    # Running workspace tests
    cargo nextest run \
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

# Run documentation tests
[group('checks')]
test-doc:
    # Running documentation tests
    __GEAR_WASM_BUILDER_NO_BUILD=1 \
    SKIP_WASM_BUILD=1 \
        cargo test --doc --workspace --no-fail-fast


# Check runtime imports
[group('checks')]
check-runtime-imports:
    # Checking runtime imports
    cargo build -p wasm-proc
    cargo build -p vara-runtime
    ./target/debug/wasm-proc \
        --check-runtime-imports \
        target/debug/wbuild/vara-runtime/vara_runtime.wasm
