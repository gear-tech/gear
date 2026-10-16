# v2.1.0 mainnet rollout: coordinator runbook

This is the mainnet coordinator checklist and handover contract for native BEEFY/MMR replacing the **Gear-to-Ethereum consensus/root proof path**. It does not replace Ethereum-to-Gear checkpoint/light-client programs, Ethereum event verification, token accounting, governance or application relayers. Retain those components until their own verified cutover. This document is not transaction or asset-movement authorization. Detailed commands are in the [mainnet operator runbook](beefy-deployment.md#mainnet-operator-runbook).

**Node installation, runtime upgrade, source BEEFY activation and Ethereum bridge cutover are four independent actions.** In particular, source BEEFY activation does not require an Ethereum deployment, destination binding or bridge-sized desired committee. Legacy GRANDPA traffic remains operational until its separate approved pause/drain/cutover.

**Unreleased quorum patch:** the quorum rules below require the patched runtime, not the published v2.1.0 WASM. The v2.1.0 version/artifact identifiers in this runbook remain the release baseline. Before enactment, the release PR must update the approved artifacts, spec version and supported migration target together and qualify that exact WASM. Installing v2.1.0 nodes alone does not change the on-chain activation rule.

## Mainnet coordinator checklist

You own the rollout ledger, validator communications and each go/no-go decision. Work in order; a checkbox means its evidence has been reviewed, not merely that a command ran. Keep one [durable release record](#evidence-status-and-handover-package); collect public evidence only, never validator session secrets, seeds or passwords. No unchecked gate is implicitly approved.

- [ ] **1. Pin the release, without retagging or rebuilding successful jobs.** v2.1.0 is pinned to `8b7dab2f89546f5ef872d9a39447a45c1d7ddc36`. Record review/final-head CI and release-job receipts against that immutable commit. Keep the release a **draft** with `make_latest=false` during preparation. Artifact/job success is not mainnet qualification. Do not move the tag, publish the draft or rebuild the successful main release as part of this preparation; any later publication requires its own authorization.
- [ ] **2. Verify artifacts and qualify enactment.** Use the [release artifact/checksum commands](beefy-deployment.md#release-ownership-and-remaining-gates). Select `production_vara_runtime_v2_01_00.wasm` and `production_vara_runtime_v2_01_00_metadata.scale`, never testnet or try-runtime WASM. Record node/metadata/WASM/SHA256SUMS identities. Require [Gate 1](#gate-1-qualify-the-mainnet-release-and-supported-state) supported-state rehearsal, exact-deployable-WASM normal-timing staging, baseline-host capacity/weights and current predecessor/owner/exposure inventory. Do not infer qualification from CI or draft assets.
- [ ] **3. Prepare operators and archives.** Inventory actual active/queued validators and electable standby operators; assign restart windows, acknowledgement channel, incident contacts and governance owners. Keep legacy bridge delivery running. Arrange at least two independent archives with state/block retention and offchain indexing before first MMR insertion **A** at upgrade. Use the [installation and proof-retention procedure](beefy-deployment.md#2-validators-and-archives-install-before-runtime-upgrade); uptime and archive flags alone do not prove historical MMR availability.
- [ ] **4. Send notice 1; install nodes BEFORE runtime enactment.** Verify the node checksum and execute the downloaded new `gear --version` on the actual host/container **before replacing the working service binary**. The released Linux binary requires **glibc 2.39** (Ubuntu 24.04 or a compatible host/container); do not blindly replace a working Debian 12/Ubuntu 22.04 service. Resolve compatibility first, without rebuilding the release. Coordinate rolling restarts, retain `--chain vara`, service/database/network identity/complete keystore/password, and preserve BABE/GRANDPA quorum. Require every duty-serving operator’s [pre-upgrade acknowledgement](vara/node/README.md#acknowledgements-to-the-coordinator). Do not rotate yet. Old nodes cannot execute the new SessionKeys ABI; the new node’s API v1 fallback only supports the predecessor before upgrade.
- [ ] **5. Recheck predecessor; enact production WASM.** Immediately before approved Root `system.setCode`, recheck authenticated four-key active ownership/order/exposures and current capacity as in [predecessor qualification](beefy-deployment.md#1-coordinator-qualify-the-supported-predecessor). Require separate mainnet governance approval; mainnet has no Sudo shortcut. Follow [upgrade inspection and finalized RPC queries](beefy-deployment.md#3-governance-enact-and-inspect-the-upgrade). Record successful finalized inner dispatch, code/metadata/checksums, unchanged `vara` genesis/identity, **11000 → 20100**, SessionKeys API **2** (`0xab3c0572291feb8b`), preserved consensus/bridge state and advancing BABE/GRANDPA finality. BEEFY remains inactive.
- [ ] **6. Send notice 2; register real native proofs.** After finalized API v2, resolve each effective signed owner from bonding and proxy/multisig context, not the outer fee payer. Operators run local `author_rotateKeysWithOwner` with that owner’s raw AccountId32 and submit ordinary `session.setKeys(keys, proof)` using the [existing generation/registration commands](beefy-deployment.md#4-validators-native-generation-and-ordinary-registration). Require 161-byte keys, nonempty 321-byte native proof, finalized successful inner dispatch and exact `Session.NextKeys(stash)` equality. Rotation replaces **all five keys**; retain all old keystore entries through duties and offence-proof/recovery retention.
- [ ] **7. Observe readiness, not a timer.** Reconcile reports at common finalized state using [actual source readiness](beefy-deployment.md#5-coordinator-verify-actual-source-readiness). Preserve complete ordered active/queued sets and check unique validators/keys, exact owner-specific placeholders, BEEFY lists, set/session mapping, current/next MMR IDs/lengths/roots and initialized history. Each actual set must be **1..1000**, with **N − floor((N−1)/3)** valid registered non-placeholder keys independently in each set. At 59 validators require at least 40; aim above the minimum for outage margin. Track electable standby readiness and future full-set quorum; unanimous registration or blanket chilling of unregistered validators is not required. Sessions are **two hours**, eras six sessions; observe actual queued/active propagation and an era transition. Registration or a key-presence RPC alone does not prove signing availability.
- [ ] **8. Schedule source BEEFY; send notice 3 with actual G.** Only after independent source approval, governance obtains Root execution of **`beefy.setNewGenesis(1)`** for initial or still-pending activation. Use the [source scheduling procedure](beefy-deployment.md#6-governance-schedule-source-beefy-independently). Record execution block **E**, finalized successful inner dispatch and `Beefy.GenesisBlock`: **G = E + 1**, not proposal/submission block plus delay. The argument-aware origin rechecks readiness at execution; record rejection instead of assuming success. A later restart after an existing start is reached retains positive checked delays and requires separate approval. No Ethereum destination/binding approval is required.
- [ ] **9. Verify native operation from independent archives.** After G, compare the same finalized checkpoints on at least two independent nodes/archives. Require cryptographically valid advancing commitments with native quorum **N − floor((N−1)/3)** (unanimous for N ≤ 3), real authority handovers and served/verified historical MMR proofs from A onward. Record hashes, set/session IDs, commitments and proof results/endpoints, not process uptime or one signature. Preserve evidence before pruning; use the [proof recovery procedure](vara/node/README.md#offline-beefy-proof-recovery) for archive loss.
- [ ] **10. Hand off source; keep bridge cutover separate.** Record source sign-off and monitoring/recovery owners. Gates [3](#gate-3-preserve-legacy-delivery-custody-and-replay-state), [4](#gate-4-bind-authenticate-and-explicitly-enable-the-destination) and [5](#gate-5-durable-operations-and-measured-acceptance) require separate destination, custody/replay, pause/drain, binding, post-binding bootstrap, accepted-root, explicit unpause and canary approvals. Source activation does not authorize bridge traffic or asset movement, and does not pause/reset the legacy bridge.

### Go/no-go and recovery forward

| Boundary | GO only when | NO-GO / recovery |
| --- | --- | --- |
| Before runtime enactment | Release identity, supported current predecessor, exact-artifact/capacity qualification, compatible duty-serving nodes, custody/indexing/archive readiness and approvals are evidenced. | Hold enactment on any missing acknowledgement, incompatible host/node, wrong artifact, ownership mismatch or lost BABE/GRANDPA finality. Preserve the working service; fix installation/qualification and recheck current finalized state. |
| After enactment, before source scheduling | API v2 receipts, actual active/queued readiness, electable standby handling and independent source approval are evidenced. | Recover forward with compatible nodes and intact databases/keystores. Keep BEEFY unscheduled until readiness is restored. Do not downgrade runtime, restore stale live state, wipe old keys, insert empty proofs or edit storage. |
| After scheduling/activation | Successful finalized scheduling, actual G and independent verified quorum/handovers/MMR availability are evidenced. | Preserve finality and evidence; isolate signing, selection, configuration or archive failure and restore that path. Do not repeatedly schedule genesis as a repair button. Later restart requires coordinated approval and proof/destination reconciliation. |
| Bridge cutover / ongoing bound traffic | Separate Gates 3–5 approvals, replay/custody reconciliation and live destination trust/progress are evidenced. | Leave destination traffic disabled if not ready. If already enabled and unsafe, explicitly pause affected paths under approved authority, stop affected writers and reconcile state. Never reset queue/nonce/MMR/client time or deploy an unrestricted fresh queue as rollback. |

### Messages to validators

Copy these notices, replacing every bracketed field from the release record. Send notices 2 and 3 only after the stated finalized evidence exists; do not present a proposal or estimated height as enactment.

**Notice 1 — install node; no rotation**

~~~text
Vara mainnet v2.1.0 — install in your assigned window; DO NOT rotate keys yet.
Release: v2.1.0 / 8b7dab2f89546f5ef872d9a39447a45c1d7ddc36
Approved manifest/download: [URL]; node/platform SHA-256: [checksum]
Restart window: [UTC window]; acknowledgement channel/contact: [channel/contact]
Verify checksum and run the downloaded gear --version before replacing the working
service binary. Linux requires glibc 2.39: use Ubuntu 24.04 or a compatible host/
container; report incompatibility before stopping/replacing the working service.
Restart the same service with --chain vara, database/network identity, complete
keystore and password configuration. Protect backups; retain every old key. Keep
authoring RPC local/access controlled and offchain indexing enabled before upgrade.
Do not purge/resync, run duplicate signers or register five-key bundles yet.
Reply with stash/effective signer, installed version/hash, mainnet genesis and
finalized block/hash, advancing BABE/GRANDPA duties and custody/indexing confirmation.
Designated archives also report retention, first-insertion coverage and endpoints.
Never send seeds, private keys, keystore files or passwords.
Procedure: vara/node/README.md#before-the-runtime-upgrade
~~~

**Notice 2 — finalized API v2; generate and register**

~~~text
Vara mainnet runtime upgrade is FINALIZED — register native keys now.
Enactment execution/finalized block/hash: [record]; production code SHA-256: [hash]
Manifest/metadata: [URL/hash]; observed vara spec 20100; SessionKeys API
0xab3c0572291feb8b = 2 at finalized hash [hash].
Resolve the effective signed account from Staking.Bonded and proxy/multisig context
(controller/represented account where applicable, NOT the outer fee payer). On
your released local node use author_rotateKeysWithOwner with its raw 32-byte
AccountId32 hex. Require 161-byte keys and nonempty 321-byte native proof; this
rotates ALL FIVE keys. Submit ordinary session.setKeys(keys, proof) from that
effective account with liquid fee funds, never an empty-proof wizard.
Reply with owner/stash, public keys/proof, finalized successful inner-dispatch
receipt and exact Session.NextKeys(stash) match to [channel/contact]. Continue
existing duties and retain old private entries through handover and retention.
Report observed queued/active session IDs and finalized hashes as they change;
two-hour sessions are not a fixed sleep-based readiness guarantee.
Electable standby operators must be ready or coordinate chilling before selection;
do not stop an active signer prematurely. Never send secrets.
Procedure: beefy-deployment.md#4-validators-native-generation-and-ordinary-registration
~~~

**Notice 3 — governance scheduled actual G**

~~~text
Vara source BEEFY activation is scheduled by FINALIZED successful Root dispatch.
Scheduling extrinsic/inner receipt and finalized block/hash: [record]
Execution block E: [E]; initial delay d = 1; verified Beefy.GenesisBlock G = E + 1: [G]
This is the execution-derived target, not a proposal height or wall-clock estimate.
Continue BABE/GRANDPA and native BEEFY duties; retain old keys and proof history.
Report signing/commitment progress, set/session IDs, real handovers and finalized
hashes to [monitoring channel/contact]; incident contact: [contact]. Independent
archives [endpoints] compare native quorum commitments and historical MMR proofs.
Do not independently schedule/restart BEEFY, bind a destination or purge keys.
Source activation does not authorize Ethereum bridge cutover or asset movement.
Procedure: beefy-deployment.md#6-governance-schedule-source-beefy-independently
~~~

The full [validator acknowledgement format](vara/node/README.md#acknowledgements-to-the-coordinator) is the handover contract. A transaction hash, process uptime or `author_hasSessionKeys=true` alone is not completion.

## Gate 1: qualify the mainnet release and supported state

Publish a mainnet manifest with immutable Gear and, for later bridge cutover, paired Bridge commits; node/runtime/metadata/contract checksums; chain spec/genesis; supported finalized predecessor; spec/transaction/API versions; normal timing/features; proof archives and independent upgrade/activation/cutover approvals. Review and CI gate merging; artifact identity gates release publication; capacity, operator readiness and governance gate enactment. Draft branch heads, historical CI totals and local artifact hashes do not approve a newer commit. Validators use released nodes and ordinary transactions, not an external session-key signer.

The exact mainnet migration is **11000 → 20100**, retaining identity `vara`. Mainnet production WASM excludes `dev`, `try-runtime` and `runtime-benchmarks`; it keeps three-second slots, two-hour sessions and six-session eras. Testnet is the same runtime with `dev`, including its identity and Sudo, and remains a build/test variant, not a required rollout stage. The release node's embedded testnet runtime does not override mainnet on-chain WASM: operators explicitly retain `--chain vara`. Use a matching same-revision mainnet try-runtime companion for the retained snapshot, and qualify the exact deployable mainnet WASM separately.

Reuse the compatible **full mainnet snapshot** at its pinned finalized state to qualify the shared native session-key migration. Preserve input-root/header comparison, exact predecessor checks, all configured try-state checks, decoding, idempotence, unsuppressed weight checks and historical ownership proofs. Pair this with the `dev` tests and normal-timing testnet build/identity/metadata/import checks; a separate testnet snapshot is not required for implementation acceptance. Keep each network's actual state inventory and deployment qualification separate. Only multi-block simulation is disabled for this single-block-only migrator. Unsupported predecessor, malformed state or oversized migration cost stops release; never alter snapshot identity to pretend it came from another network.

### Preserved-state contract

The migration preserves four legacy public fields and ordered active/queued sets, key ownership, **bridge pause state, queue, nonce, owners and history**. It adds deterministic `0x02 || Keccak256(stash.raw32)` BEEFY placeholders, not usable signing keys. It does not pause/reset/bind the bridge or activate BEEFY. Missing inactive BEEFY bookkeeping is initialized without overwriting existing records. First-upgrade `Beefy.GenesisBlock` remains absent.

The current retained **four-key historical root and validator count must authenticate actual active authority ordering, keys and active-era exposures before any migration write**. Absence of a pending/stalled GRANDPA change is not sufficient. Delayed authority divergence or a stale key-only-rotation commitment stops enactment; never recover attribution from mutable registrations. Follow the [predecessor qualification and recheck procedure](beefy-deployment.md#1-coordinator-qualify-the-supported-predecessor).

Prior historical roots stay unchanged. Current/queued five-key roots are rebuilt while original roots remain available for already-issued proofs during historical retention; pruning still ends that availability. Immutable active snapshots, not mutable `NextKeys`, define later historical roots. Preserve the full keystore and old private entries through handover and recovery/offence-proof retention.

No persistent custom ownership ledger or pending-owner counter gates activation. Registered-owner migration input is not limited by either the source's 1000 committee bound or destination's 256 capacity. Compare live owner/exposure counts with qualified migration bounds before enactment; half-block trie headroom is **plus size-dependent database charges**, not a fixed total migration ceiling.

### Node/API ordering and native registration

Install/restart compatible nodes **before** runtime upgrade, preserving database/service/keystore/password configuration and BABE/GRANDPA quorum. Old nodes are incompatible with the changed runtime ABI. New-node API v1 fallback only supports the four-key runtime **before** upgrade, not old-node operation after upgrade.

Enable offchain indexing before the first MMR insertion and establish independently operated archives. Archive flags cannot backfill pruned state or missing offchain MMR nodes. First insertion **A** is distinct from later BEEFY genesis **G**.

After upgrade, query finalized `state_getRuntimeVersion`/metadata and require SessionKeys API **2**, ID **`0xab3c0572291feb8b`** (pinned SDK Blake2b-64 hash of `SessionKeys`). Resolve `Staking.Bonded(stash)` and the effective signed origin. On the local released node call `author_rotateKeysWithOwner` with that account's **raw 32-byte AccountId32 hex**, not SS58 or a SCALE Vec prefix. Require returned 161-byte keys and nonempty **321-byte native proof**, then submit `session.setKeys(keys, proof)` with ordinary metadata-aware transaction tooling and verify successful finalized inner dispatch and exact `NextKeys` equality. Do not submit legacy empty proofs or export secrets.

Native generation uses **seed None** and rotates **all five** session keys. Every key proves `POP_ || owner`; the protocol is not genesis-/whole-bundle-bound. Old private entries remain necessary during active/queued handover. Custom keystore path/password must match on restart; password affects derivation, not file encryption. Protect filesystem/backups separately. Signing failure fails generation, but generation is nontransactional and unused private entries may remain. Presence RPCs alone do not establish actual native signing.

## Gate 2: independently activate source BEEFY

At a common finalized hash check complete ordered active/queued bundles against current/next BEEFY lists, unique validators/keys, exact owner-specific placeholders, session mapping and exact current/next MMR IDs/lengths/roots, initialized nonzero MMR history and actual propagation. Each set independently needs **N − floor((N−1)/3)** valid registered non-placeholder keys, with N equal to that full set’s size. Placeholder slots are never removed or counted as registered, including placeholders that happen to be valid curve points. Any other malformed key rejects readiness. The source readiness bound remains **1..1000 actual active and queued authorities**, independent of binding, desired `Staking.ValidatorCount` and dormant owners.

Encourage active, queued and electable standby operators to register native keys, but activation does not require every validator to register. There is no smaller committee, new election filter or forced chilling. Monitor signing margin and the full next elected sets; coordinate existing staking operations if a future set would lose quorum without prematurely stopping active signers. Dormant non-electable owners need not return/purge. Registration cannot guarantee future private-key availability. Observe real normal-timing sessions/era changes, not a fixed delay or key-presence-only RPC.

After source approval, Root governance schedules **`beefy.setNewGenesis(1)`** for initial or still-pending activation, so the target is exactly the execution block plus one. This prevents a distant initial target from outliving the checked active/queued sets. After an existing start has been reached, an approved restart may use another positive non-overflowing delay and must still pass readiness. The call remains Operational with the source-1000 reservation and argument-aware origin, including bypass dispatch. It neither requires bridge binding nor resets public-chain genesis/MMR/bridge state. Require finalized dispatch, advancing cryptographically verified native commitments, actual authority handovers and historical MMR availability.

**Native quorum is `N − floor((N−1)/3)` over the full set, including placeholders; N ≤ 3 is unanimous.** For 59 validators, 40 registered signers are the minimum; 42 give two-signature outage margin. The queued set has its own denominator (60 requires 41). Destination signature sampling remains distinct and unchanged: the `floor(N/3)+1` cap and fixed 86/86 floors select 20 at 59 authorities, 51 at 150 and 86 at 256. Interactive delay/window remain 128/24 destination blocks. Deploy the paired relayer patch preserving original indices and canonical empty leaves for invalid curve points; never substitute zero addresses or compact the set. The destination ABI and MMR snapshot version do not change.

## Gate 3: preserve legacy delivery, custody and replay state

Source migration/activation is not approval to replace the destination. Keep legacy GRANDPA operation until the separately reviewed cutover. Define and approve a finalized cutoff; pause source and destination/application paths as required, stop legacy writers, drain/reconcile pending roots/messages, processed nonces, supplies, balances, custody and governance references. **Source pausing does not invalidate already authenticated legacy deliveries**; control destination processing independently.

Prefer storage preservation of an existing authorized MessageQueue proxy when feasible. It has no public verifier setter: switching requires a reviewed governance-authorized mechanism, not another `initialize`, invented setter or raw storage write. Verify deployed-state storage layout, duplicate-root maturity/conflicting-root behavior, root availability, original maturity, replay state and pending admin/user deliveries. A BEEFY adapter does not repair those legacy queue semantics by itself.

A fresh production queue starts with empty consumed-nonce state. Since `EthMessageExt::hash` still hashes legacy nonce/source/destination/payload, it can replay old consumed messages attested under the new domain. An explicit replay/custody/application authorization migration is mandatory; do not operate unrestricted old/new queues for the same assets. Queue pause, root challenge, emergency stop and application pause are distinct controls.

The client/adapter have immutable bindings and no resetter. Qualify expiry/irrecoverable-mismatch recovery with authorization reachable independently of an expired verifier. Replacement client/adapter/queue is a state-reconciliation operation, not a time reset or fresh deployment shortcut.

## Gate 4: bind, authenticate and explicitly enable the destination

1. Verify reviewed v2-compatible destination/relay deployment, bytecode, governance, queue/client/chain bindings and source/destination identities. Actual current and queued committees and desired `Staking.ValidatorCount` must each fit **256**. Larger source committees may keep native BEEFY while destination traffic is disabled.
2. Bind once **while paused**, through approved Root `gearEthBridge.bindDestination`: actual nonzero source genesis, nonzero 32-byte big-endian Ethereum chain ID, approved nonzero 20-byte queue. Domain is `Keccak256("vara/gear-eth-bridge-domain/v2" || sourceGenesis[32] || chainIdBE[32] || queue[20])`. Binding may precede or follow source activation; verify finalized tuple/domain/event and prohibit rebinding.
3. Obtain a signed **post-binding** newest leaf under the exact domain, compare one finalized checkpoint on independent sources, authenticate bootstrap, initialize/verify destination and continue all intervening handovers. A pre-binding leaf is not readiness evidence. Require a later accepted nonzero MMR root before root registration.
4. After drain/replay/custody/destination approvals, explicitly `gearEthBridge.unpause` through authorized governance. It remains **Normal**, using a separate full-bridge **256-authority allowance**, not source Operational/1000 reservation. Verify finalized dispatch, canary maturity/delivery/replay and then approve assets; retain reverse-direction services.

### Paired protocol invariants

- Source snapshot is **86 bytes**, `2 || "vara" || bridgeDomain[32] || parentTimestampLE[8] || initialized[1] || queueIdLE[8] || root[32]`; its Keccak hash is leaf-extra. Native commitments/version-0 113-byte outer MMR leaf stay unchanged. Historical v1 destination deployments do not prove v2 compatibility.
- Canonical v1 queue envelope is `576 + 32N` bytes with at most 256 proof items; v0 is not a compatibility path.
- Discover A (first MMR insertion) and G (BEEFY genesis). Authenticate finalized bootstrap C after A/G and binding, with `0 < A < C <= u32::MAX`, real parent C−1 timestamp, exact ordered current/next tuples and next ID = current ID + 1. Never substitute synthetic time or arbitrary roots.
- New client starts with zero accepted MMR root; a later signed accepted update is required. Every update authenticates the newest leaf, even within the same set. Source time is nondecreasing, at most 120 seconds ahead, and expires strictly after 24 hours; receipt time does not renew trust and an expired client cannot self-revive.
- Historical queue times are not independently expired but proofs require the latest nonzero anchor of a live client. Clearing a source queue does not erase authenticated history. Adapter is immutable for its client, destination chain and queue; direct operator calls cannot replace queue registration.

## Gate 5: durable operations and measured acceptance

Qualify normal-timing existing-state upgrade and sustained operation for the mainnet artifacts before enactment. Reuse the retained mainnet state and shared-migration evidence; public-testnet deployment is not a prerequisite. Fresh-chain tests alone do not prove an existing-mainnet upgrade or operational readiness.

Relay acceptance includes durable accepted checkpoints/cursors/transaction records, canonical-inclusion reconciliation after reorgs, reconnect/catch-up of mandatory handovers, archive-loss/outage visibility, fee-payer nonce ownership and Ethereum replacement/receipt/finality reconciliation. Capture root/message proofs before source rollover, rebuild stale anchors against the latest accepted root, and verify restart without resetting either chain. Alert well before freshness expiry. Integrate real token accounting and reverse-direction services, not only a mock receiver.

Bound source admission fails closed on malformed/missing identity, absent/future BEEFY start, inconsistent MMR/descriptors/session mapping, or actual/desired >256. Scheduling a future restart blocks bound admission **immediately**. It does not automatically flip pause or reset queue/nonce/history/destination. Per-message checks are structural/capacity checks, not full committee signature/private-key checks. On unsafe liveness explicitly pause/reconcile and verify new native finality/destination progress before reopening.

Lowering desired count alone cannot shrink actual/queued >256; same-committee sessions may retain the old oversized set. Observe suitable actual handover/commitments rather than assuming recovery. BEEFY-only rotation preserves queue; actual GRANDPA changes retain delayed rollover. Pending-clear admission rejects all senders, including governance, preserving message/fee state; retain applicable historical and GRANDPA evidence under existing clear/reset rules.

The [validator baseline and weight-qualification contract](beefy-deployment.md#hardware-and-weight-qualification) are unchanged. Maintainers qualify source-1000 Operational readiness, full bridge-256 Normal unpause, full-value oversize rejection, migration growth and complete transition-block hooks with production max analysis, **50 steps / 20 repeats**, raw JSON and conservative margins. Preserve upstream base weights and the documented additional allowances. Local measurements do not qualify the baseline host; link reviewed measurements from the durable release record.

## Evidence status and handover package

Keep current qualification results in the [durable release record](beefy-deployment.md#evidence-status-and-handover), not as historical success counts in this procedure. Compatible predecessor snapshots remain reusable; rerun a changed migration against the retained supported mainnet state. Baseline-host capacity gates mainnet enactment; independent destination approvals gate the later bridge cutover. An older PR approval or local artifact hash does not approve a newer revision.

A completed handover contains the fields below. Keep each phase **pending** until evidence is attached and its accountable approver signs off; use **not applicable** with a reason for deferred bridge work. This procedure asserts no live mainnet enactment, source activation, destination deployment or completed qualification.

| Release-record fields | Required contents |
| --- | --- |
| Release identity / job ledger | v2.1.0 immutable commit; reviewed Gear/paired Bridge pins where applicable; review/final-head CI links; workflow name, run URL/ID, attempt, inputs and conclusion for each required release job; artifact/platform filenames, SHA-256/SHA256SUMS and download URLs; draft/publication status and approval owner. Preserve successful run receipts; publication is separate. |
| Mainnet manifest | Actual source genesis/chain spec; node version and host/container compatibility; production WASM/metadata identities; predecessor/runtime spec, transaction/API versions; feature/timing configuration; archive endpoints and incident owners. |
| Qualification | Retained snapshot checksum, finalized block/hash/header root and predecessor identity; same-revision try-runtime companion checksum; migration/decoding/try-state/idempotence/weight and historical ownership-retention evidence; current owner/exposure inventory; baseline-host measured capacity/weight margins; exact-artifact normal-timing registration/restart/session/era evidence and approver. |
| Operator readiness | Stash/effective raw AccountId32; installed node/version/hash and custody/indexing acknowledgement; public keys/proof and registration extrinsic/finalized inner receipt; exact NextKeys equality; observed active/queued session IDs/hashes and native BEEFY key; standby readiness/chilling disposition; old-key retention confirmation. Never attach secrets. |
| Upgrade / source activation | Separate approvals; runtime execution/finalized block/hash, code/metadata/API observations and preserved-state comparison; scheduling extrinsic/inner receipt, execution E, d > 0, computed/stored G; first MMR insertion A; independent checkpoint/commitment/quorum/proof results, set/session IDs and handovers; source sign-off and monitoring/recovery owners. |
| Later bridge cutover | Separate destination/custody/replay approvals; reviewed deployment bytecode/ownership/configuration; binding/domain and finalized binding/unpause receipts; signed post-binding bootstrap, later accepted roots/times and handovers; legacy drain/processed-nonce/custody/application reconciliation; matured canary/token delivery/replay evidence in both directions. |
| Durable operations / retirement | Relay configuration, accepted cursors/transactions, fee-payer nonce ownership, restart/reorg/outage recovery evidence, proof archives and freshness monitoring; explicit sign-off for retiring only the replaced Gear-to-Ethereum path while retaining reverse-direction services. |

Abort on wrong identity/binding/bytecode, source disagreement, invalid sets/signatures, missing archives/handovers, expired trust or unreconciled messages/assets. Preserve evidence and stop affected writers. After user execution, recovery is state reconciliation: old binaries, stale databases, reset client time, wiped MMR or queues are not rollback.

Resolve Bridge references against its reviewed manifest revision: `ethereum/src/beefy/BeefyClient.sol`, `ethereum/src/VaraQueueRootVerifier.sol`, `ethereum/src/MessageQueue.sol`, and `tools/beefy-relay/src/`. Source counterparts include `vara/runtime/vara/src/{beefy_activation.rs,bridge_leaf.rs,session_history.rs,migrations/session_keys.rs}`, the local session/BEEFY patches and bridge pallet. See [patch provenance](substrate/README.md) and [node procedure](vara/node/README.md#beefy-upgrade-and-later-activation).
