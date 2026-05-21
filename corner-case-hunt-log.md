# Corner-case vulnerability hunt log

Branch: `gsobol/ethexe/corner-case-hunt` (off `gsobol/ethexe/malachite-new`).
Goal: find latent vulnerabilities / corner-case bugs in the ethexe-malachite
layer through targeted unit tests. Each iteration: invent one hypothesis,
write a test, run it. If the test PASSES (no bug reproduces) — delete the
test. If it FAILS (bug suspected) — verify the test is correct and KEEP it
(marked `#[ignore]`) as a bug record.

## Skip list — already known / fixed / tracked

Do NOT re-test these areas. They are pinned in memory
`ethexe-malachite-pending-fixes.md`.

### Fixed (do not re-test)

| Area | Fix commit |
|---|---|
| `app.rs:115-149` StartedRound remove-before-validate | `f3c5639a1` |
| `app.rs:handle_app_msg` `?`-propagation kills app task | `cacf41ac1` |
| `app.rs:process_finalized` partial-finalize drift | `0ef199abd`, `cc3f4e3c6`, `e81a572c0` |
| `app.rs:process_received_proposal_part` future-height unbounded buffer | `42a0d6024` (FUTURE_HEIGHT_WINDOW = 4) |
| `externalities.rs:validate_block_above` quarantine-poll | `6d302a7a0` (post_quarantine_delay) |
| `externalities.rs:validate_block_above` missing strict-descendant | `1052391fa` |
| `mempool.rs:purge_expired` unresolved ref_block DoS | `d52c62e01` |
| `mempool.rs:purge_expired` drops unknown ref_block — ACCEPTED tradeoff: insert tolerates "ref_block not in local DB yet" but purge_expired evicts on next `set_chain_head`. SDK must set `ref_block ≤ head-1`. Do NOT test this asymmetry as a fresh bug. | (documented in iter #4 — already covered by issue #9 fix policy) |
| `codec.rs:From<RawProposedValue>` Round::Nil aliasing | `503a3d43d` (TryFrom) |

### Known-open follow-ups (tracked as GitHub issues — do NOT add new tests for these)

| Issue | Area |
|---|---|
| #5473 | `PartStreamsMap` unbounded growth + caps |
| #5474 | Mempool per-signer quota |
| #5475 | Per-peer rate limit on `process_received_proposal_part` |
| #5476 | `ProposalFin` signature check before buffering future-height parts |
| #5477 | Shared helper for producer/validator EB-advance |
| #5478 | Upper-bound validation on `post_quarantine_delay` |
| #5479 | Metrics for `validate_block_above` abstains |
| #5480 | Validator peer-id allowlist |
| #5481 | Multi-validator integration test for `post_quarantine_delay` lagging observer |
| #5482 | Misc polish: chain_head==None test + TryFrom round-bound test + mempool insert doc |

## Iteration history

Format: each entry is one row in the table below. Add new entries APPEND-ONLY
(newest at bottom).

| # | UTC timestamp | Hypothesis | Area / file | Test name | Outcome | Notes |
|---|---|---|---|---|---|---|
| 0 | 2026-05-20T21:00:00Z | seed | — | — | — | log initialized |
| 1 | 2026-05-21T08:55:00Z | validate_block_above lacks per-MB injected-tx size cap that build_block_above enforces — relies on 1MB Malachite hard cap (~8x looser than 127KB protocol cap) | ethexe/malachite/service/src/externalities.rs:557-560,584-590 | validate_rejects_mb_exceeding_injected_size_cap | abandoned | tmpfs /tmp full (6/7.5GB), rocksdb cc build OOM disk-quota. Couldn't compile to verify within budget. Hypothesis stands on code-reading: validator checks shape+quarantine+TxValidity+touched-cap but NOT cumulative `tx.encoded_size()` sum. Worth re-running with target on /home. |
| 2 | 2026-05-21T09:15:00Z | mempool accepts txs whose reference_block height > chain_head height; tx_validity.rs:184 rejects them — capacity DoS via unfetchable future-anchored txs | ethexe/malachite/service/src/mempool.rs:773-810 (insert_should_reject_future_ref_block) | insert_should_reject_future_ref_block | bug-found | `is_expired(head, ref)` is `ref + WINDOW <= head` — false when `ref > head`. mempool.insert returns Ok for ref_block at height 100 while head is 2. Such tx is unfetchable (not in `recent_ancestors`) AND would be rejected by consensus `is_reference_block_within_validity_window` which requires `ref_height <= head_height`. Test marked #[ignore]. Mempool insert path should mirror the consensus rule. |
| 3 | 2026-05-21T10:00:00Z | streaming.rs `StreamState::insert` overwrites `total_messages` on every `Fin`, distinct from #5473's unbounded growth: a second `Fin` at a lower sequence lowers the completion target. Attacker (proposer of the stream) sends Init + N Data + Fin@K (legit), then a second Fin@(N+1) — `buffer.len() == total_messages` fires while genuine Data parts at seqs N+1..K are still missing. | ethexe/malachite/core/src/streaming.rs:99-115 | streaming::tests::double_fin_with_smaller_sequence_completes_stream_prematurely | bug-found | Test FAILS (bug reproduced): sequence Init@0, Data@1, Data@2, Data@3, Fin@100, Fin@5 — the second Fin overwrites `total_messages = 6` and `buffer.len() == 6` ⇒ `is_done()` true, stream emits truncated `ProposalParts`. Marked `#[ignore]`. Fix: lock `total_messages` after first `Fin` OR require any subsequent `Fin` to carry the same sequence. |
| 4 | 2026-05-21T09:21:18Z | mempool `insert` deliberately accepts txs whose ref_block hasn't yet replicated to the local DB (comment at mempool.rs:298-301: "best-effort: filters at fetch time once the block lands locally"), but `purge_expired` — fired on every `set_chain_head` — treats unknown ref_block as expired and drops the tx. So the insert tolerance is undone by the very next block tick. | ethexe/malachite/service/src/mempool.rs:232-263 | mempool::tests::purge_expired_must_not_evict_unknown_ref_block_within_grace | bug-found | Test FAILS (bug reproduced): insert tolerates unknown ref_block (pool.len()==1); set_chain_head(next EB) immediately purges it (pool.len()==0). Race: RPC accepts the client's promise, observer ticks once, promise is silently orphaned. Marked `#[ignore]`. Fix: purge_expired should retain unknown-ref_block entries that arrived within a grace window of `latest_head_height`, mirroring insert's tolerance. |
| 5 | 2026-05-21T11:30:00Z | StreamState::insert ties Init extraction to `msg.is_first()` (= sequence == 0). A Byzantine peer can place a Data part at seq 0 and the actual Init at seq 1: init_info is never populated, `is_done()` blocks on `init_info.is_some()`, and the (peer, stream_id) slot is held forever even after all parts + Fin arrive — distinct from #5473 (general unboundedness) and from iter#3 (double-Fin). | ethexe/malachite/core/src/streaming.rs:90-115 | streaming::tests::init_at_non_zero_sequence_never_completes | bug-found | Test FAILS (bug reproduced): sequence Data@0, Init@1, Data@2, Fin@3 → buffer has 4 entries, total_messages=4, but init_info stays None ⇒ is_done false ⇒ stream never removed from PartStreamsMap. A single Byzantine peer can convert each opened stream into a permanently held slot with just 4 messages. Marked `#[ignore]`. Fix: extract Init by content kind (`p.as_init()`), independent of sequence position; or reject seq-0 messages whose content isn't `ProposalPart::Init` as a protocol violation and drop the state. |
| 6 | 2026-05-21T12:05:00Z | Retry of iter#1 with disk-space resolved: `validate_block_above` (externalities.rs:546-560) deliberately omits the per-MB cumulative-encoded-size cap that `build_block_above` enforces (externalities.rs:321-326). Comment justifies omission by appealing to Malachite's ~1 MiB block-payload hard cap — i.e. validator accepts ~8x the producer-side 127 KiB budget. | ethexe/malachite/service/src/externalities.rs:2027-2127 (validate_rejects_mb_exceeding_injected_size_cap) | externalities::tests::validate_rejects_mb_exceeding_injected_size_cap | bug-found | Test FAILS (bug reproduced): two max-payload injected txs (cumulative 258452 bytes ≈ 252 KiB) targeting two distinct initialized destinations both fully pass `TxValidityChecker` and the touched-programs cap. `validate_block_above` returns Ok(true) even though MAX_INJECTED_TRANSACTIONS_SIZE_PER_MB = 130048 bytes (127 KiB). A malicious proposer can inflate `compute_mb`'s injected-message work to 8x the protocol budget per MB. Marked `#[ignore]`. Fix: add cumulative `tx.encoded_size()` sum check on the validator side mirroring `build_block_above`'s producer-side logic, returning Ok(false) when the running total exceeds MAX_INJECTED_TRANSACTIONS_SIZE_PER_MB. |
| 7 | 2026-05-21T09:52:00Z | `forget()` stamps every committed tx into `seen` with its `reference_block`. If that ref_block isn't in the local DB (validator's observer lags the producer), the very next `set_chain_head` runs `purge_expired`, whose `seen.retain` falls through to `_ => false` for unknown ref_block → seen entry evicted → dedup gate gone → same network-committed tx can be re-inserted. Symmetric to iter #4 but on the forget→purge path. | ethexe/malachite/service/src/mempool.rs:232-263 (purge_expired) and 859-940 (test) | mempool::tests::forget_then_purge_evicts_seen_entry_for_unknown_ref_block | bug-found | Test FAILS (bug reproduced): forget(tx with unknown ref_block) → set_chain_head fires purge_expired → seen entry evicted → re-insert returns Ok(()) instead of Err(AlreadyCommitted). A re-submitted tx can re-enter the pool after the network already committed it. Marked `#[ignore]`. Fix: in `purge_expired`'s seen-retain loop, treat `None` (unknown ref_block) as "keep" — same tolerance the insert path extends; only evict when ref_block is known AND past the validity window. |
| 8 | 2026-05-21T10:05:00Z | Cheapest-possible stuck-stream attack: a peer sends a SINGLE `Fin@0` message with no payload at all. `is_first()` true but `as_data()` is None (Fin content) → `init_info` stays None; `fin_received` flips true; `total_messages = 1`; buffer pushes the Fin → `buffer.len() == 1`. `is_done()` blocks on `init_info.is_some()` ⇒ slot parked indefinitely. 1:1 message-to-stuck-slot amplification — strictly cheaper than iter #3 (5 msgs), iter #5 (4 msgs), or #5473's attacks (≥2 msgs). | ethexe/malachite/core/src/streaming.rs:90-115 (StreamState::insert / is_done) | streaming::tests::lone_fin_at_seq_zero_holds_slot_forever | bug-found | Test FAILS (bug reproduced): a single `fin_msg(s, 0)` insert leaves `PartStreamsMap.streams` non-empty with no completion possible — `init_info` can never become `Some` since `seen_sequences` already contains 0. Distinct defect from iter #5: that case had Init at a non-zero seq (recoverable by extracting Init by content kind); this case has NO Init anywhere in the stream, so the only safe fix is to detect "complete-by-counters but no Init" as a malformed stream and drop the state. Marked `#[ignore]`. |
