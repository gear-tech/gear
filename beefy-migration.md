# Mainnet migration to native BEEFY

This is the mainnet coordinator checklist and handover contract for native BEEFY/MMR replacing the **Gear-to-Ethereum consensus/root proof path**. It does not replace Ethereum-to-Gear checkpoint/light-client programs, Ethereum event verification, token accounting, governance or application relayers. Retain those components until their own verified cutover. This document is not transaction or asset-movement authorization. Detailed commands are in the [mainnet operator runbook](beefy-deployment.md#mainnet-operator-runbook).

**Node installation, runtime upgrade, source BEEFY activation and Ethereum bridge cutover are four independent actions.** In particular, source BEEFY activation does not require an Ethereum deployment, destination binding or bridge-sized desired committee. Legacy GRANDPA traffic remains operational until its separate approved pause/drain/cutover.

## Mainnet coordinator checklist

You own the rollout ledger, validator communication and the go/no-go decision at each boundary. Keep one durable mainnet release record; collect public evidence only, never validator session secrets, seeds or passwords.

| Stage | Your action | Required acknowledgement before proceeding |
| --- | --- | --- |
| Finalize the PR | Resolve review findings; include every native SDK patch, lockfile and documentation change in the reviewed commit. Exclude unrelated local work. Require review and CI on that exact head, not an older approved revision. | Final head SHA, resolved findings and green required checks. |
| Merge and release | Merge through the repository policy; tag the approved merged commit. Run the existing Release workflow for that tag, initially as a draft with `make_latest=false`. Verify the mainnet WASM, metadata, node and `SHA256SUMS`, then publish and distribute the release record. | Published immutable tag/commit and verified artifact identities. Do not publish from an uncommitted working tree. |
| Prepare mainnet | Retain the mainnet snapshot evidence, complete baseline-capacity and exact-artifact qualification, record live predecessor/validator inventories, and schedule a coordinated upgrade window. Keep legacy bridge delivery running. | All active/queued operators and electable standby operators accounted for; proof archives ready from the first MMR insertion; no unresolved capacity or finality issue. |
| Roll out nodes | Send the published node and [validator checklist](vara/node/README.md#beefy-upgrade-and-later-activation). Coordinate rolling restarts while preserving BABE/GRANDPA quorum. | Every required operator reports the pre-upgrade acknowledgement below; old node versions are not left serving post-upgrade duties. |
| Enact runtime | Arrange approved mainnet Root execution of `system.setCode` using the production WASM. Record execution/finalization and inspect migrated state using the runbook. | Finalized `vara/20100`, expected code/metadata, SessionKeys API v2, preserved old consensus/bridge state and ongoing BABE/GRANDPA finality. |
| Register and propagate keys | Tell validators to generate native owner-bound bundles locally and submit ordinary `session.setKeys`. Reconcile receipts at common finalized state; observe actual active/queued propagation and standby readiness. | Public registration acknowledgements, exact active/queued readiness across normal sessions/era and retained old keys; no placeholder signer remains in either source committee. |
| Activate source BEEFY | Submit separately approved Root `beefy.setNewGenesis(delayInBlocks > 0)`. Record G from the execution block, then monitor native quorum, real handovers and archive proofs. | Successful finalized inner dispatch and advancing verified BEEFY commitments/MMR proofs from independent nodes. No Ethereum binding is required. |
| Cut over the bridge later | Follow Gates 3–5 only after destination review and approval. Coordinate pause/drain/reconciliation, immutable binding, post-binding bootstrap, accepted roots and explicit unpause/canary. | Separate custody/replay/destination sign-off. Source activation alone is not permission to move bridge traffic or assets. |

Before enactment, stop the rollout on a missing operator acknowledgement, incompatible node, wrong artifact or lost BABE/GRANDPA finality. After enactment, recover forward with compatible binaries; do not downgrade the runtime, restore stale live databases, wipe keys or reset queues/MMR. If source readiness fails, leave BEEFY unscheduled; if destination readiness fails, leave destination traffic disabled. A future BEEFY restart is a coordinated operation, not a repair button.

### Messages to validators

Send three explicit notices, with the pinned mainnet manifest and your acknowledgement channel:

1. **Install now; do not rotate yet.** Give the approved node checksum, restart window and requirement to preserve the existing service/database/keystore/password. Collect installed version/hash, stash/effective owner, mainnet genesis/finalized height, indexing and custody confirmation.
2. **Runtime upgrade is finalized; register now.** Give the finalized enactment block/code identity and API v2 confirmation. Ask for local `author_rotateKeysWithOwner`, ordinary signed registration and the public receipt/bundle; never ask validators to send private keys.
3. **Source activation is scheduled.** Give the finalized scheduling receipt and actual G, monitoring contacts and retention requirement. Collect live duty/hand-over observations. Validators must not independently schedule genesis, bind a destination or purge keys to make a checklist green.

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

At a common finalized hash check ordered active/queued bundles against current/next BEEFY lists, valid unique non-placeholder keys and validators, session mapping and exact current/next MMR IDs/lengths/roots, initialized nonzero MMR history and actual propagation. The source readiness bound is **1..1000 actual active and queued authorities**, independent of binding, desired `Staking.ValidatorCount` and dormant owners.

Active, queued and **electable standby** operators rotate native keys. An unready electable operator chills through existing staking; there is no new election filter or forced chilling. Dormant non-electable owners need not return/purge. Ownership proof on registration cannot guarantee private-key availability for a future elected candidate. Observe real normal-timing sessions/era changes, not a fixed delay or key-presence-only RPC.

After source approval, Root governance schedules `beefy.setNewGenesis(delayInBlocks > 0)` for a checked future execution-block-plus-delay target. The call is Operational with a separate source-1000 reservation and readiness in the argument-aware origin, including bypass dispatch. It neither requires bridge binding nor resets public-chain genesis/MMR/bridge state. Require finalized dispatch, advancing cryptographically verified native commitments, actual authority handovers and historical MMR availability across independent nodes.

**Native quorum is `N − floor((N−1)/3)`; N ≤ 3 is unanimous.** Destination signature sampling is distinct: existing paired policy has the `floor(N/3)+1` cap and fixed 86/86 floors (20 selected at 59 authorities, 51 at 150, 86 at 256). Check the reviewed artifact's Fiat-Shamir/interactive constants, not a blanket one-third formula. Interactive delay/window remain 128/24 destination blocks.

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

A completed handover contains:

- Reviewed immutable Gear/Bridge pins, mainnet artifacts/checksums/metadata/manifest and separate upgrade, activation and cutover approvals.
- Supported-state migration/ownership retention, baseline capacity/weight evidence and normal-timing native registration/session/restart qualification.
- Finalized upgrade/activation/binding/unpause dispatch receipts; deployed code/ownership/bindings; signed post-binding bootstrap and subsequent accepted roots/times/handover proofs.
- Legacy drain/processed-nonce/custody/application reconciliation and matured canary/token delivery/replay evidence in both directions.
- Durable relay configuration/recovery/monitoring, accountable fee-payer ownership, proof archives and explicit sign-off for retiring only the replaced Gear-to-Ethereum path.

Abort on wrong identity/binding/bytecode, source disagreement, invalid sets/signatures, missing archives/handovers, expired trust or unreconciled messages/assets. Preserve evidence and stop affected writers. After user execution, recovery is state reconciliation: old binaries, stale databases, reset client time, wiped MMR or queues are not rollback.

Resolve Bridge references against its reviewed manifest revision: `ethereum/src/beefy/BeefyClient.sol`, `ethereum/src/VaraQueueRootVerifier.sol`, `ethereum/src/MessageQueue.sol`, and `tools/beefy-relay/src/`. Source counterparts include `vara/runtime/vara/src/{beefy_activation.rs,bridge_leaf.rs,session_history.rs,migrations/session_keys.rs}`, the local session/BEEFY patches and bridge pallet. See [patch provenance](substrate/README.md) and [node procedure](vara/node/README.md#beefy-upgrade-and-later-activation).
