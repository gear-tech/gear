A fuzzying-like framework to run checks of various invariants defined on the chain state.

Based on `cargo fuzz` intergrated with the `libfuzzer` library.

Documentation available [here](https://rust-fuzz.github.io/book/introduction.html).

<br/>

## Install cargo-fuzz

```bash
cargo install cargo-fuzz
```

## Run fuzz targets

Running the default target (`utils/economic-checks/fuzz/fuzz_targets/simple_fuzz_target.rs`) is as simple as

```
make fuzz
```

To choose an arbitrary target, run

```bash
./scripts/gear.sh test fuzz ${TARGET_NAME}
```
The corresponding target source file `${TARGET_NAME}.rs` must be present in `utils/economic-checks/fuzz/fuzz_targets` folder.
