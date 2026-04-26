# gas-bench

Deterministic gas-burn benchmark harness for Gear programs. Used to A/B compare optimization changes — pre/post a code change, you get exact gas numbers and any non-zero delta is real.

For *what* to optimize and how to interpret the numbers, see [OPTIMIZATION_GUIDE.md](OPTIMIZATION_GUIDE.md).

## Build

```
cargo build -p gas-bench --release
```

Binary: `target/release/gas-bench`.

## Scenarios

| Scenario | What it exercises | Demo wasm |
|----------|-------------------|-----------|
| `async-common` | 3× `send_for_reply` + reply (async runtime, `load_bytes`, `handle_reply_with_hook`) | `demo-async` |
| `async-mutex` | `Mutex::lock().await` + cross-program call | `demo-async` |
| `sync-ping` | Pure sync handle path; control baseline | `demo-ping` |
| `state-heavy-transfer` | Init + populate N balances + measure single Transfer (lazy-page-heavy) | `demo-fungible-token` |

## A/B comparison recipe

```sh
# Build the wasm on each branch
git switch <baseline> && cargo build -p demo-async --release
cp target/wasm32-gear/release/demo_async.opt.wasm /tmp/baseline.wasm

git switch <candidate> && cargo build -p demo-async --release
cp target/wasm32-gear/release/demo_async.opt.wasm /tmp/candidate.wasm

# Run the same scenario against each
./target/release/gas-bench async-common --wasm /tmp/baseline.wasm
./target/release/gas-bench async-common --wasm /tmp/candidate.wasm
```

Diff the `total_gas_burned`. Numbers are deterministic across runs of the same wasm — any non-zero delta is a real cost change. Run each side three times to confirm.

## State-heavy scenario

```sh
./target/release/gas-bench state-heavy-transfer \
  --wasm target/wasm32-gear/release/demo_fungible_token.opt.wasm \
  --accounts 500
```

Reports `setup_gas_burned` (init + populate N balances) **separately** from `total_gas_burned` (the measured single Transfer). Vary `--accounts` to see how lazy-page work scales with state size.

## Output

```
scenario:           async-common
wasm:               /tmp/candidate.wasm
messages_processed: 1
total_gas_burned:   1258717195
per_message:
  [ 0] 1258717195
```

`gas_burned` is in raw gas units. To convert to weight, multiply by ~300 (the Vara runtime weight-to-gas factor; for exact value see `runtime/vara/src/lib.rs`).

## Reference baselines

Numbers from `master` as of this branch's creation (release `.opt.wasm`):

| Scenario | Total gas |
|----------|----------:|
| `sync-ping` | 510,819,090 |
| `async-common` | 1,261,258,987 |
| `state-heavy-transfer --accounts 100` | 670M setup / 468M transfer |
| `state-heavy-transfer --accounts 1000` | 3,416M setup / 573M transfer |

Use these as sanity checks: if your local `master` build doesn't match within ~0.5%, something has drifted (toolchain, profile, or instrumentation version).

## Adding a scenario

A scenario is a function in `src/scenarios.rs` returning `ScenarioResult`. The pattern:

1. Build a `System`, mint balance to `USER_ID`
2. Deploy the program (and any mock counter-parties via `WasmProgram` trait)
3. Run setup messages with `drain_blocks_silent(&sys)` if you want them excluded from the measured total
4. Send the message that exercises the path you care about
5. Call `drain_blocks(&sys)` and return the result

Then add a `Scenario` variant in `src/main.rs` and dispatch to your function.

Keep scenarios small and orthogonal. One scenario per workload pattern, not per contract.
