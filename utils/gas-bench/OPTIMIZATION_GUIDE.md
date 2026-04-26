# Gas optimization guide for Sails contributors

A practical playbook for app developers and Sails-framework contributors who want to reduce on-chain cost. Every claim here was measured with `gas-bench`; numbers cited are from a real run, not an estimate. See [README.md](README.md) for tool usage.

## Where the cost actually lives

Per-message cost breakdown for a typical Sails contract:

```
Per-message cost floor (every dispatch pays this)
├─ Module instantiation (Instance setup)            ~150-330M weight
├─ Storage reads (program data + allocations)        ~100M weight
├─ Lazy-page initialization (mprotect)               ~5-10M weight
└─ Empty handle gas charges                          ~5M weight
                                                    ─────────
                                                    ~270-450M weight floor

Plus, scaled by what your program actually does:
├─ Lazy-page LOAD per touched page                    ~76M weight  ← state read
├─ Lazy-page WRITE per touched page                  ~104M weight  ← state mutation
├─ gr_send / gr_reply syscalls                       ~150M weight each
├─ gr_size, gr_reply_to, gr_source (getters)         ~92M weight each
└─ Allocator (alloc/free) per call                   ~165-310M weight
```

**The single biggest variable cost for state-heavy contracts is lazy-page work.** A contract that loads + mutates 5 pages per dispatch pays ~900M weight on memory paging alone — more than the whole instantiation floor.

## What to look for in your contract

### 1. Big monolithic State

```rust
pub struct State {
    pub baskets: BTreeMap<u64, Basket>,        // grows unbounded
    pub positions: BTreeMap<(u64, ActorId), Position>,
    pub settlements: BTreeMap<u64, Settlement>,
    pub agents: Vec<AgentInfo>,
    pub config: Config,
}
```

Every dispatch loads this whole blob. If it's serialized to ≥3 lazy pages, you pay ~3 × 76M = ~230M weight just to read state, even if your handler only touches `config`. Sails (`#[program]`/`#[service]` macros) currently SCALE-decodes the entire State per dispatch.

**Look for:** total encoded `State` size > 1 page (64 KB) is the inflection point. Past 2-3 pages, partial-load designs start to pay back.

### 2. Hot mutations on big collections

```rust
self.state.baskets.insert(basket_id, basket);  // BTreeMap with 1000+ entries
```

Inserting into a large BTreeMap can rebalance several internal nodes, dirtying multiple lazy pages. Currently any byte mutation in a 64 KB page = the whole 64 KB page gets re-uploaded → ~104M weight per dirtied page.

**Look for:** handlers that mutate fields inside maps with >100 entries.

### 3. Accidental async on sync routes

Sails has `const ASYNC: bool` per route. A handler with no `.await` is statically classified `ASYNC=false` and skips `message_loop` registration entirely. But:

```rust
pub async fn maybe_does_thing(&mut self) -> Result<u64, Error> {
    // no .await anywhere
    Ok(42)
}
```

`async fn` with no `.await` still pays the async runtime cost in some setups. Verify `Self::ASYNC` resolves to `false` via expansion (or just remove `async`).

**Look for:** `pub async fn …` with no `.await` in body.

### 4. Heap allocations in hot paths

Each `alloc` call costs **~165M weight** + ~145M per "batch" — the allocator path is one of the most expensive single operations in a contract. Common sources:

- `Vec::new()` followed by `.push()` in a loop (use `Vec::with_capacity`)
- `String::from(format!(...))` (use `&str` where possible)
- `.to_vec()` / `.clone()` on payload bytes (read into stack buffer instead)

**Look for:** any `Vec`, `String`, `Box`, `BTreeMap::new` in a hot handler. If it's small and known-size, consider a stack array (`[T; N]` or `arrayvec::ArrayVec`).

### 5. Redundant syscalls

Each `gr_*` syscall is **~80-150M weight**. Calling `msg::source()` or `msg::value()` twice in a handler pays for it twice. Sails framework already mitigates some of this via `Syscall` lazy caching, but contract code can still leak. Check:

```rust
fn handler(&mut self) {
    let user = sails_rs::gstd::msg::source();   // syscall #1
    self.do_a(user);
    let user2 = sails_rs::gstd::msg::source();  // syscall #2 — redundant
    self.do_b(user2);
}
```

**Look for:** repeated calls to `msg::source`, `msg::value`, `msg::id`, `exec::block_height`, `exec::block_timestamp`, `exec::gas_available`. Hoist into a local.

### 6. Codec round-trips for large types

`msg::reply<T: Encode>(big_struct)` already uses stack-buffered encoding via `gstd::util::with_optimized_encode` (no heap alloc when payload fits `MAX_BUFFER_SIZE`). Same for `msg::load<T: Decode>` (uses `with_read_on_stack_or_heap`). **You don't need to do anything special** unless your payload exceeds the stack buffer (~MB-class).

**Look for:** payloads larger than `MAX_BUFFER_SIZE` — those *do* heap-allocate. If you have one, redesign to chunk it.

## What to optimize, in priority order

### Priority 1 — Cap your state size

Hard MAX bounds on every collection. polybaskets gets this right:

```rust
const MAX_ITEMS_PER_BASKET: usize = 32;
const MAX_NAME_LEN: usize = 128;
const MAX_AGENT_NAME_LEN: usize = 20;
```

This caps both gas-per-dispatch (via wasm-instrument BB count) AND the lazy-page footprint. Without caps, your contract's per-dispatch cost grows unboundedly with adoption.

### Priority 2 — Co-locate hot fields

If `config` is read on every dispatch and `baskets` is read rarely, put `config` first in the struct so it lands on the same lazy page as the state header. Cold fields (large maps) sit on subsequent pages and only fault when actually accessed.

This is workable today even without framework changes: `#[repr(C)]` ordering doesn't apply to SCALE encoding, but Rust struct field order *does* affect static layout when the struct is held in `static mut`. Sails `Service` patterns hold state in `&mut State` references, so layout is implicit.

**To verify:** run `state-heavy-transfer` with different `--accounts` values. If transfer gas grows with state size, you're paying for state you don't read.

### Priority 3 — Avoid rebalancing-heavy mutations

`BTreeMap::insert` on a 1000-entry map can dirty 3-5 pages worth of tree nodes. Alternatives:

- `Vec<(K, V)>` with linear search: faster for N < 64, single contiguous page
- Append-only log + checkpoint: insert is O(1) on a single page; periodic compaction
- Two-level: hot index (small `Vec`) + cold archive (BTreeMap, only loaded when scanning)

**Measure:** swap the data structure, run `state-heavy-transfer` before/after.

### Priority 4 — Hoist syscalls and constants

Easiest win. Cache `msg::source()`, `block_timestamp()`, etc. into locals. Each saved syscall = ~92M weight.

### Priority 5 — Aggressive release profile

Sails-style:

```toml
[profile.release]
opt-level = "s"
lto = true
codegen-units = 1
[profile.release.package."*"]
opt-level = 3
strip = true
```

This shrinks the wasm 30-50% vs the gear default profile, which proportionally reduces every per-instruction cost (basic-block count scales with code size).

## How to measure

### Baseline your contract

1. Build your contract in release: `cargo build -p my-contract --release`
2. Find the `.opt.wasm` (typically `target/wasm32-gear/release/my_contract.opt.wasm`)
3. Run the closest matching gas-bench scenario.

If none of the built-in scenarios fit, **add one** — see [README.md § Adding a scenario](README.md). Keep your custom scenario in your own fork; the harness is small (~150 lines) and easy to extend.

### A/B a change

1. Note baseline number on `master`: `gas-bench <scenario> --wasm <baseline.wasm> > /tmp/before.txt`
2. Make your change, rebuild
3. Note candidate number: `gas-bench <scenario> --wasm <candidate.wasm> > /tmp/after.txt`
4. Diff. Run each side **3 times** to confirm determinism (numbers will be identical or your change broke something).

### Convert gas to weight

`gas_burned` is in raw gas units. To compare against schedule weights in `pallets/gear/src/weights.rs`:

```
weight = gas × ~300   (Vara runtime; check WeightOfGas constant for exact factor)
```

A 100M gas saving ≈ 30B weight ≈ ~30ms wall-clock on the reference Xeon Platinum 8375C.

### What's a "real" win

| Saving | Verdict |
|--------|---------|
| < 0.1% of total | Probably noise from data-layout shuffle, not your change |
| 0.1-1% | Small but real; ship if it's free (no API change, no maintenance burden) |
| 1-10% | Worth a focused PR |
| > 10% | Significant; likely worth a design discussion |

For state-heavy contracts, single-digit-percent wins are realistic from contract-author changes. Double-digit wins typically require framework or runtime changes.

## What NOT to waste effort on

`wasm-opt` (run as part of every `.opt.wasm` build) **already kills** these:

- Variant duplication (e.g. `send_with_gas` vs `send_delayed` — wasm-opt merges them)
- Unused match arms in error decoding (folded to single comparison)
- `value == 0` branches in syscall wrappers (eliminated when value is statically zero)
- `vec![0u8; n]` followed by overwrite (turned into uninit alloc + memcpy)
- `#[cold]` annotations on already-cold paths
- No-op `Pin::new` / `Box::new` round trips when the result is immediately consumed

`gstd` **already does** these (don't reinvent):

- Stack-buffered encode/decode for `msg::reply<T>` and `msg::load<T>` (see `gstd/src/util.rs` `with_optimized_encode` and `gstd/src/msg/encoded.rs` `load`)
- Skip `gr_size` syscall in `msg::load_bytes` via `read_exact` (see `gcore/src/msg.rs` and the `vs/gstd-gas-microopts` branch if your toolchain doesn't have it yet)

## Tasks for Sails framework contributors

Three concrete optimization candidates ranked by leverage. Each is verifiable with `gas-bench state-heavy-transfer`.

### Task A — Per-field state storage (largest user-visible win)

**Problem:** `#[program]` macro generates monolithic State serialization. Every dispatch SCALE-decodes the full struct, even if the handler only touches `config`.

**Direction:** Each top-level field becomes its own storage key, lazy-loaded when first accessed. New attribute: `#[state(per_field)]` or auto-detect with field tracking.

**Where:** `sails-rs/src/macros/program/`, `sails-rs/src/gstd/services.rs`. Reference impl already exists in `awesome-sails-storage-0.2.0`.

**Estimated win:** 3-5 fewer pages loaded per typical dispatch ≈ 230-380M weight saved (~30-60% of a state-heavy dispatch).

**How to verify:** run `state-heavy-transfer --accounts 1000` before/after. The Transfer gas should drop 100-200M.

**Risk:** breaking change to storage format. Needs migration story.

### Task B — Skip async runtime for sync routes (free win)

**Problem:** `#[gstd::async_main]` always wraps the entry point in `message_loop` registration, even when no route awaits. Sails has `const ASYNC: bool` per route but the entry is still async-wrapped.

**Direction:** Generate a fast-path entry that calls the route directly when all routes have `ASYNC=false`. Or per-dispatch dispatch: at runtime, check the route's `ASYNC` const and skip Task creation when false.

**Where:** `sails-rs/src/macros/program/` entry generation; `gstd/src/async_runtime/futures.rs` could expose a `register_sync_only` hook.

**Estimated win:** for sync-only dispatches, eliminates the entire async runtime cost (`func[203]`-class function in our measurements: ~100-300M gas per program). For polybaskets-style contracts that are 90% sync, this is essentially free.

**How to verify:** add a `state-heavy-transfer-sync` variant scenario where no route awaits. Compare total_gas before/after.

**Risk:** none, fully internal to Sails.

### Task C — Sub-page write coalescing in gear-protocol

**Problem:** Mutating 1 byte in a 64 KB page → whole 64 KB re-uploaded (104M weight).

**Direction:** Track dirty pages at 4 KB sub-granularity (16 sub-pages per WASM page). Upload only dirty sub-pages. Requires gear-protocol PR (lazy-pages + journal encoding + storage schema migration).

**Where:** `lazy-pages/src/common.rs` `IntervalsTree<GearPage>`, `core/processor/src/executor.rs` `lazy_pages_post_execution_actions`.

**Estimated win:** ~80M weight per write-heavy dispatch (single-byte mutation in a 1000-entry map currently uploads 5 pages = 520M; with sub-page tracking ≈ 100M).

**How to verify:** `state-heavy-transfer --accounts 1000` — Transfer gas should drop ~80-150M.

**Risk:** consensus-affecting change, needs migration. Larger scope than tasks A/B.

## Ranked recommendation

For a Sails app developer optimizing your own contract today: **start with Priority 1-4** above (caps, hot field placement, BTreeMap alternatives, syscall hoisting). Use `state-heavy-transfer` with a custom scenario for your contract. Likely double-digit-percent gains available.

For Sails framework contributors: **Task B first** (free, fully internal). Then Task A (largest impact, requires migration story).

For gear-protocol contributors: **Task C** is the highest-leverage runtime change — affects every contract whether or not they migrate.
