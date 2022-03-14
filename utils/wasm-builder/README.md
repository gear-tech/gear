# Gear WASM Builder

This is a helper crate that can be used in build scripts for building Gear programs.

## Usage

1. Add the `gear-wasm-buider` crate as a build dependency to the `Cargo.toml`:

```toml
# ...

[build-dependencies]
gear-wasm-builder = "0.1.2"

# ...
```

2. Create a `build.rs` file and place it at the directory with `Cargo.toml`:

```rust
fn main() {
    gear_wasm_builder::build();
}
```

3. Use `cargo` as usually:

```bash
cargo clean
cargo build
cargo build --release
cargo test
cargo test --release
```

4. Find the built WASM binaries in `target/wasm32-unknown-unknown/<profile>` directory:

- `.wasm` — original WASM built from the source files
- `.opt.wasm` — optimised WASM binary to be submitted to the blockchain
- `.meta.wasm` — metadata providing WASM binary for auxiliary purposes

5. Also, you can include a generated `wasm_binary.rs` source file to use the WASM code while e.g. writing tests.

```rust
mod code {
    include!(concat!(env!("OUT_DIR"), "/wasm_binary.rs"));
}

#[test]
fn debug_wasm() {
    assert_eq!(
        std::fs::read("target/wasm32-unknown-unknown/debug/test_program.wasm").unwrap(),
        code::WASM_BINARY,
    );
    assert_eq!(
        std::fs::read("target/wasm32-unknown-unknown/debug/test_program.opt.wasm").unwrap(),
        code::WASM_BINARY_OPT,
    );
    assert_eq!(
        std::fs::read("target/wasm32-unknown-unknown/debug/test_program.meta.wasm").unwrap(),
        code::WASM_BINARY_META,
    );
}
```

## License

Source code is licensed under `GPL-3.0-or-later WITH Classpath-exception-2.0`.
