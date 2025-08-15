# Lazy Pages Fuzzer

A lightweight, parallel, deterministic fuzzer for exercising `lazy-pages` functionality. It can run normally to search for failures, and it can reproduce any failure exactly using a recorded **instance seed** together with a persisted **base seed** file.

## Run

Run the fuzzer:

```bash
RUST_LOG=info cargo run --release -p lazy-pages-fuzzer-runner -- run
```

On the first run, a `seed.bin` file is created in the **current working directory** and used as the base seed for fuzzing.  
On subsequent runs, this file is **not** overwritten. To start with a new base seed, manually remove or rename the existing `seed.bin`.

> **Note:** The fuzzer attempts to use all available CPU cores, which can significantly increase system load.

## Reproduce

When a failure is found, the fuzzer prints the **instance seed** to `stderr` (with other failure related info).

To reproduce a failure, you need **both** of the following:

- **Base seed** — the original `seed.bin` from the run where the failure occurred.
- **Instance seed** — the 64-character hex string printed to `stderr` at the time of the failure.

Steps:

1. Copy the original `seed.bin` into your current working directory.
2. Run the fuzzer with the `reproduce` command, passing the instance seed:

```bash
RUST_LOG=debug cargo run --release -p lazy-pages-fuzzer-runner -- reproduce 8b7f0e0b3a1b4c2d5e6f7a8b9c0d1e2f11223344556677889900aabbccddeeff
```
