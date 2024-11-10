# runtime-fuzzer

This is an internal product that is used to find panics that may occur in our runtime.

### Measuring code coverage

Pre-requirements:

- llvm-tools: `rustup component add llvm-tools`
- [cargo-fuzz](https://github.com/rust-fuzz/cargo-fuzz): `cargo install cargo-fuzz`
- [cargo-binutils](https://github.com/rust-embedded/cargo-binutils): `cargo install cargo-binutils`
- [rustfilt](https://github.com/luser/rustfilt): `cargo install rustfilt`

Running fuzzer on the local machine:

```bash
cd utils/runtime-fuzzer

# Fuzzer expects a minimal input size of 350 KiB. Without providing a corpus of the same or larger
# size fuzzer will stuck for a long time with trying to test the target using 0..100 bytes.
mkdir -p fuzz/corpus/main
dd if=/dev/urandom of=fuzz/corpus/main/fuzzer-seed-corpus bs=1 count=350000

# Run fuzzer for at least 20 minutes and then press Ctrl-C to stop fuzzing.
# You can also remove RUST_LOG to avoid printing tons of logs on terminal.
RUST_LOG=debug,syscalls,runtime::sandbox=trace,gear_wasm_gen=trace,runtime_fuzzer=trace,gear_core_backend=trace \
cargo fuzz run \
    --release \
    --sanitizer=none \
    main \
    fuzz/corpus/main \
    -- \
        -rss_limit_mb=8192 \
        -max_len=450000 \
        -len_control=0

# Get coverage
cargo fuzz coverage \
    --release \
    --sanitizer=none \
    main \
    fuzz/corpus/main \
    -- \
        -rss_limit_mb=8192 \
        -max_len=450000 \
        -len_control=0
```

### Viewing code coverage

There are two ways to view coverage:

- in text mode

  ```bash
  # generate `coverage.txt`
  HOST_TARGET=$(rustc -Vv | grep "host: " | sed "s/^host: \(.*\)$/\1/")
  cargo cov -- show target/$HOST_TARGET/coverage/$HOST_TARGET/release/main \
      --format=text \
      --show-line-counts \
      --Xdemangler=rustfilt \
      --instr-profile=fuzz/coverage/main/coverage.profdata \
      --ignore-filename-regex=/rustc/ \
      --ignore-filename-regex=.cargo/ &> fuzz/coverage/main/coverage.txt
   ```

- in Visual Studio Code
  with [Coverage Gutters](https://marketplace.visualstudio.com/items?itemName=ryanluker.vscode-coverage-gutters):

  ```bash
  # generate `lcov.info` file with coverage
  HOST_TARGET=$(rustc -Vv | grep "host: " | sed "s/^host: \(.*\)$/\1/")
  cargo cov -- export target/$HOST_TARGET/coverage/$HOST_TARGET/release/main \
      --format=lcov \
      --instr-profile=fuzz/coverage/main/coverage.profdata \
      --ignore-filename-regex=/rustc/ \
      --ignore-filename-regex=.cargo/ > fuzz/coverage/main/lcov.info
  ```

  Then you need to install the Coverage Gutters extension and use Ctrl-Shift-P to invoke the "Coverage Gutters: Watch"
  action. The extension will look for the `lcov.info` file. After a while, the coverage will appear in your editor.
