# Check all code `(default)`
all: native wasm

# Check all native code
native:
    # Checking all native code with Clippy
    __GEAR_WASM_BUILDER_NO_BUILD=1 SKIP_WASM_BUILD=1 \
      cargo clippy \
        --locked --workspace --all-targets --all-features \
        -- --no-deps -D warnings

# Check all WASM code
wasm:
    # Checking all WASM code with Clippy
    cargo metadata --no-deps --format-version=1 \
        | jq -r '.packages.[] \
            | select(.features | index("std")) \
            | "-p=" + .name' \
        | RUSTFLAGS="--cfg=substrate_runtime" \
          __GEAR_WASM_BUILDER_NO_BUILD=1 SKIP_WASM_BUILD=1 \
            xargs sh -c 'cargo clippy "$@" \
                --locked \
                --no-default-features \
                --target=wasm32v1-none \
                -- -D warnings'

# Check examples
examples: examples-native examples-wasm

# Check native parts of examples
examples-native:
    # Checking native parts of examples with Clippy
    __GEAR_WASM_BUILDER_NO_BUILD=1 SKIP_WASM_BUILD=1 \
      cargo clippy -p 'demo-*' -p 'test-syscalls' \
        --locked --all-targets --all-features \
        -- --no-deps -D warnings

# Check WASM parts of examples
examples-wasm:
    # Checking WebAssembly parts of examples with Clippy
    cargo metadata --no-deps --format-version=1 \
        | jq -r '.packages.[] \
            | select(.manifest_path \
            | contains("gear/examples")) \
            | select(.dependencies.[].name == "gear-wasm-builder") \
            | "-p=" + .name' \
        | __GEAR_WASM_BUILDER_NO_BUILD=1 SKIP_WASM_BUILD=1 \
            xargs sh -c 'cargo clippy "$@" \
                --locked \
                --no-default-features \
                --target=wasm32v1-none \
                -- -D warnings'
