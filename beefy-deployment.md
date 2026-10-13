# Mainnet BEEFY deployment and migration

## Mainnet operator runbook

This runbook is for the **mainnet 11000 → 20100** four-key migration, preserving runtime identity `vara` and the existing chain. **Installing the node, upgrading the runtime, activating source BEEFY, and cutting over the Ethereum bridge are four separate actions.** Source activation requires neither a destination binding nor bridge approval. Publishing a release performs none of these actions. Start with the [coordinator checklist](beefy-migration.md#mainnet-coordinator-checklist); validators follow the [node checklist](vara/node/README.md#beefy-upgrade-and-later-activation).

The selected rollout is mainnet only. The retained mainnet snapshot qualifies the shared migration, with separate `dev` feature tests/build checks; a public-testnet deployment is not a prerequisite. Never substitute testnet keys, genesis hashes, runtime artifacts, bindings or checkpoints. An unsupported mainnet predecessor is a stop, not permission to edit storage. This document does not authorize transactions.

### Release ownership and remaining gates

| Owner | Required evidence |
| --- | --- |
| Release coordinator (you) | Reviewed immutable commit, mainnet node/runtime/metadata artifact SHA-256 checksums, chain spec/genesis, finalized supported predecessor, spec/transaction/API versions, normal timing/features, archive endpoints and separate upgrade/activation/cutover approvals. Publish through the existing release pipeline. |
| Runtime maintainers | Reuse the supported full mainnet snapshot for the shared migration implementation, preserving its finalized input root and migration/idempotence/decoding/try-state/weight evidence. Check the `dev` variant separately through tests and network-specific build/metadata/import checks; retain per-network state inventory, exact-artifact staging and baseline-host capacity qualification. |
| Validators / coordinator | Compatible nodes installed before enactment, protected persistent keystores, finalized native registration and actual active/queued propagation; readiness of electable standby operators, not mandatory return of all dormant owners. |
| Governance | Separately approved network runtime upgrade and source activation; successful finalized inner dispatch, not merely an included wrapper. Bridge pause/binding/unpause have separate approvals. |
| Bridge operators / reviewers | Reviewed v2-compatible destination/relay, deployed configuration/bootstrap verification, legacy drain/replay/custody reconciliation and authorized recovery before opening traffic. |

Production-profile artifacts are a [release-workflow](.github/workflows/release.yml) responsibility, not a validator build task. The workflow checks out its `tag_name` input: create the approved tag on the final merged commit before dispatching it. Prefer a draft with `make_latest=false`, verify the artifacts/checksums, then publish deliberately. Publication is not runtime enactment. `2_01_00` means on-chain spec **20100**; verify actual published filenames.

Use the published `gear` node with **`--chain vara`**, `production_vara_runtime_v2_01_00.wasm` and `production_vara_runtime_v2_01_00_metadata.scale`, checked against `SHA256SUMS`. The runtime must exclude `dev`, `try-runtime` and `runtime-benchmarks`. Do not upload `testnet_vara_runtime_v*.wasm` or a try-runtime companion.

**Linux binary compatibility:** v2.1.0 is built on Ubuntu 24.04 and requires GLIBC 2.39. Before stopping the old service, run the downloaded executable with `--version` on each validator host. Debian 12 / Ubuntu 22.04 hosts cannot run this prebuilt binary directly; qualify a compatible host or the release container first.

### Recovering release packaging without rebuilding the runtime

The Docker workflow authenticates to GitHub to download `gear` and `SHA256SUMS`, verifies the binary, and copies it into an Ubuntu 24.04 image. It supports draft assets without putting a GitHub token in the image. The main Release workflow disables downstream `latest` promotion for drafts; direct Docker dispatches reject promotion of a draft. Run `.github/workflows/CI-docker-gear.yml` with the approved release tag and `latest=false` to repair packaging; use the reviewed workflow branch as `--ref` until the fix is merged. Do not rerun the main Release build, move the release tag, or promote `latest` to repair a container job.

Multiplatform packages are produced separately in `gear-tech/builds` and published to its download service. For a failed full run, rerun its failed jobs in the same run so publication retains successful outputs. Use a separate targeted dispatch only when the other required packages are already published; it does not import another run's artifacts. Follow that repository's recovery instructions. Missing Telegram notification secrets skip optional notifications, not download, checksum, build, push or scan failures. Publish the GitHub draft and promote distribution aliases only after their separate checks and approval.

Inspect the approved release tag through the release API; this lists artifacts,
not an approval to use a different or latest release. Verify downloaded node, WASM
and metadata files against the approved mainnet checksum manifest. API asset digests,
where present, do not replace that manifest. Run the shell examples in Bash; stop
on any failed command or JSON-RPC error before proceeding.

~~~bash
set -euo pipefail
: "${RELEASE_TAG:?Set the approved immutable release tag}"
curl --fail --silent --show-error \
  "https://api.github.com/repos/gear-tech/gear/releases/tags/$RELEASE_TAG" \
  | jq -e '{tag: .tag_name, target: .target_commitish, assets: [.assets[] | {name, digest, url: .browser_download_url}]}'
: "${NODE_ARTIFACT:?Set the downloaded node artifact}" "${NODE_SHA256:?Set its approved SHA-256}"
printf '%s  %s\n' "$NODE_SHA256" "$NODE_ARTIFACT" | sha256sum --check -
: "${RUNTIME_ARTIFACT:?Set the correct network WASM}" "${RUNTIME_SHA256:?Set its approved SHA-256}"
printf '%s  %s\n' "$RUNTIME_SHA256" "$RUNTIME_ARTIFACT" | sha256sum --check -
: "${METADATA_ARTIFACT:?Set the matching mainnet metadata}" "${METADATA_SHA256:?Set its approved SHA-256}"
printf '%s  %s\n' "$METADATA_SHA256" "$METADATA_ARTIFACT" | sha256sum --check -
~~~

Mainnet uses normal three-second slots, two-hour sessions and six-session/twelve-hour eras. The release node intentionally embeds the testnet runtime; mainnet nodes execute the actual on-chain mainnet WASM, selected with `--chain vara`. Use a same-revision mainnet try-runtime companion only for migration APIs, never as upgrade WASM. Byte identity across profiles is not assumed.

### 1. Coordinator: qualify the supported predecessor

Pin finalized state. Inventory all `Session.NextKeys` owners, `Session.Validators`, `Session.QueuedKeys`, four-key ownership, historical roots/exposures and bridge state. Check exact spec **11000**, matching network name and four-key encoding. Reuse an existing compatible full snapshot with its original block/hash/root; rerun the changed runtime against it. Archive inputs and new logs/proof-retention evidence durably; ephemeral paths are not release archives.

Migration CI downloads the retained mainnet snapshot from `https://archive.vara.network/runtime.snap` and verifies the SHA-256 pinned in `.github/workflows/build.yml`; it does not scrape live state on every run. If the hosted file changes, CI must fail checksum verification. Qualify the replacement with the full migration checks before updating the pin, and retain its finalized block/header root and predecessor identity with the release evidence.

**Authenticate active ownership before enactment.** The current four-key `HistoricalSessions` root and validator count must match the reconstructed active authority lists and active-era exposures. A delayed GRANDPA change can invalidate positional reconstruction even after `PendingChange` and `Stalled` disappear; predecessor key-only rotations can also leave a stale commitment. Either mismatch rejects preflight and halts migration before writes. Do not infer owners from mutable `NextKeys`, edit the snapshot or bypass this check.

Recheck this condition immediately before approved upgrade execution and coordinate against concurrent key registrations or forced GRANDPA transitions. If it fails, postpone enactment until a finalized state authenticates the active snapshot, then qualify that state. A retained snapshot that passes remains reusable, but does not certify later live state. Include the additional current four-key trie construction in migration capacity/weight qualification.

Testnet uses the same runtime with `dev` enabled. The native session-key migration and BEEFY/bridge readiness implementation are shared, so the retained mainnet snapshot rehearsal supplies their full-state migration evidence; a separate public-testnet snapshot is not an implementation acceptance gate. Existing `dev` tests and normal-timing testnet artifact checks cover the feature variant. This does not equate chain state or approvals: `dev` includes Sudo and testnet identity, while the mainnet migration tuple additionally includes the builtin ED-lock migration. Keep network-specific inventories and deployment qualification separate.

Using pinned try-runtime CLI **0.10.1**:

~~~bash
set -euo pipefail
: "${SNAPSHOT:?Select the retained supported mainnet snapshot}" "${TRY_RUNTIME_WASM:?Select the matching mainnet companion}"
test -f "$SNAPSHOT" || exit 1
try-runtime --runtime "$TRY_RUNTIME_WASM" on-runtime-upgrade \
  --blocktime 3000 --checks all --disable-mbm-checks snap -p "$SNAPSHOT"
~~~

Match the unmodified input root to the finalized header. Keep spec, decoding, idempotence and unsuppressed weight checks. Only multi-block simulation is disabled: this runtime has no multi-block migrator and that simulation fabricates the predecessor version. Oversized state must fail qualification. Scoped externalities or fresh-chain smoke results do not prove an existing-network upgrade or live finality.

### 2. Validators and archives: install before runtime upgrade

Verify release node checksums/version and coordinate rolling restart without losing BABE/GRANDPA quorum. Preserve service account, chain spec, database/base path, network identity, keystore path and password configuration. Do not purge/resync, reset genesis or run concurrent signing nodes with the same keys.

**ABI warning:** old nodes are incompatible with the changed SessionKeys v2 runtime ABI. The new node's API v1 fallback only supports the four-key predecessor **before upgrade**; it does not let old nodes run the new runtime. Do not register five-key/v2 bundles on the predecessor.

Back up/protect the **entire existing keystore** and password under established custody procedures. Retain all old private entries through active/queued handover and offence-proof/recovery retention. Keep unsafe authoring RPC loopback-only/access controlled; retain validator mode, GRANDPA and BEEFY networking.

Enable `--enable-offchain-indexing true` **before upgrade/first MMR insertion**. Arrange at least two independent proof servers with `--state-pruning archive --blocks-pruning archive`, indexed from the first insertion. Validators need not all become archives. Flags cannot restore pruned state or backfill offchain MMR nodes; late servers need verified replay/recovery. Compare the same finalized hashes and exercise historical proof serving, not just uptime.

For offline block archive recovery, use the [BEEFY proof-retention and interrupted-import procedure](vara/node/README.md#offline-beefy-proof-recovery). A successful block import is not proof that offchain MMR history or live finality has been restored; verify both before returning the archive to service.

### 3. Governance: enact and inspect the upgrade

Through the existing mainnet governance route, obtain Root execution of `system.setCode` with the approved production WASM; mainnet has no Sudo shortcut. After execution, verify finalized code/checksum, unchanged `vara` identity, spec **20100**, metadata/API versions and successful inner dispatch. Keep BABE/GRANDPA and the legacy bridge operating.

Migration preserves four public-key fields, validator/queued ordering and key ownership; appends deterministic placeholder `0x02 || Keccak256(stash.raw32)`; preserves **bridge pause state, queue, nonce, owners and history**; and does not automatically pause, bind, clear or reset the bridge. Older historical roots stay unchanged; original current/queued roots remain usable for issued ownership proofs while their sessions are retained, alongside rebuilt five-key roots. Missing inactive BEEFY bookkeeping is initialized without replacing existing records or activating BEEFY. First-upgrade `Beefy.GenesisBlock` remains absent. Placeholders are not operational signers.

At common finalized state, use approved metadata-aware query tooling to read `Staking.Bonded(stash)` and determine the **effective signed account** executing Session: distinct controller where applicable, or represented proxy/multisig account, not the outer fee payer. Session stores keys under the converted validator/stash. Reconcile absent/changed bonding; never guess the signer.

Read-only HTTP JSON-RPC examples (Bash, curl and jq):

~~~bash
set -euo pipefail
: "${SOURCE_HTTP:?Set the approved mainnet HTTP RPC}"
SPEC_NAME=vara
FINALIZED_HASH="$(curl --fail --silent --show-error -H 'Content-Type: application/json' \
  --data '{"jsonrpc":"2.0","id":1,"method":"chain_getFinalizedHead","params":[]}' \
  "$SOURCE_HTTP" | jq -er '.result | select(test("^0x[0-9a-fA-F]{64}$"))')"
curl --fail --silent --show-error -H 'Content-Type: application/json' \
  --data "{\"jsonrpc\":\"2.0\",\"id\":2,\"method\":\"state_getRuntimeVersion\",\"params\":[\"$FINALIZED_HASH\"]}" \
  "$SOURCE_HTTP" | jq -e --arg spec "$SPEC_NAME" \
  '.result | select(.specName == $spec and .specVersion == 20100) | select(any(.apis[]; .[0] == "0xab3c0572291feb8b" and .[1] == 2))'
curl --fail --silent --show-error -H 'Content-Type: application/json' \
  --data "{\"jsonrpc\":\"2.0\",\"id\":3,\"method\":\"state_getMetadata\",\"params\":[\"$FINALIZED_HASH\"]}" \
  "$SOURCE_HTTP" | jq -er '.result | select(type == "string" and length > 2)'
~~~

The SessionKeys API ID **`0xab3c0572291feb8b`** is derived by the pinned SDK runtime-API macro: eight-byte Blake2b hash of `SessionKeys`. Require advertised API **2**, not merely spec 20100. Decode metadata in the approved query/signing tool. Also query `chain_getBlockHash` with `[0]` and compare actual genesis to the manifest.

### 4. Validators: native generation and ordinary registration

After finalized API v2 is live, use the released node's local unsafe **`author_rotateKeysWithOwner`**. Set `OWNER` to the actual effective signer's **raw 32-byte AccountId32 hex**: not SS58 text or a length-prefixed SCALE Vec. Production generation uses the configured keystore and **seed None**, not a deterministic development seed.

~~~bash
set -euo pipefail
: "${OWNER:?Set the actual effective raw AccountId32 hex}"
[[ "$OWNER" =~ ^0x[[:xdigit:]]{64}$ ]] || exit 1
NATIVE_KEYS="$(curl --fail --silent --show-error -H 'Content-Type: application/json' \
  --data "{\"jsonrpc\":\"2.0\",\"id\":4,\"method\":\"author_rotateKeysWithOwner\",\"params\":[\"$OWNER\"]}" \
  http://127.0.0.1:9944 | jq -e '.result | select(.keys | type == "string" and test("^0x[0-9a-fA-F]{322}$")) | select(.proof | type == "string" and test("^0x[0-9a-fA-F]{642}$"))')"
KEYS="$(printf '%s' "$NATIVE_KEYS" | jq -er '.keys')"
PROOF="$(printf '%s' "$NATIVE_KEYS" | jq -er '.proof')"
curl --fail --silent --show-error -H 'Content-Type: application/json' \
  --data "{\"jsonrpc\":\"2.0\",\"id\":5,\"method\":\"author_hasSessionKeys\",\"params\":[\"$KEYS\"]}" \
  http://127.0.0.1:9944 | jq -e '.result == true'
~~~

Use the actual service port. Require **161 public-key bytes** (BABE/GRANDPA/ImOnline/AuthorityDiscovery/BEEFY order) and **321 native proof bytes**, the five-signature tuple. Every key signs `POP_ || owner` with its application scheme. ECDSA possession uses normal Blake2-based signing, not Keccak/prehashed BEEFY commitment signing; noncanonical high-S possession is rejected. Native proofs are owner-bound, not genesis-/whole-bundle-bound. Never use `author_rotateKeys` plus `0x` proof, concatenate keys manually or export session secrets.

This rotates **all five keys**, not only BEEFY. Migration's preservation of four public fields does not mean later native rotation preserves them. Keep all old private entries through duty and retention periods. In an approved metadata-aware ordinary extrinsic interface, submit **`session.setKeys(keys, proof)`** from the effective owner, with liquid unbonded funds for fees; use the generic interface if a wizard sends empty proofs. For proxy/multisig check the represented origin. Verify finalized dispatch, including wrapper results, and exact `Session.NextKeys(stash)` equality.

The operator's approved transaction tool can produce an ordinary signed extrinsic for RPC transport (this command neither signs nor checks finalization):

~~~bash
set -euo pipefail
: "${SIGNED_EXTRINSIC:?Set the approved signed session.setKeys extrinsic}"
[[ "$SIGNED_EXTRINSIC" =~ ^0x[[:xdigit:]]+$ ]] || exit 1
curl --fail --silent --show-error -H 'Content-Type: application/json' \
  --data "{\"jsonrpc\":\"2.0\",\"id\":6,\"method\":\"author_submitExtrinsic\",\"params\":[\"$SIGNED_EXTRINSIC\"]}" \
  http://127.0.0.1:9944 | jq -er '.result'
~~~

A hash is not successful finalized registration. Retain finalized receipt and public bundle/proof, never secrets. No separate key signer, code download or manually built bundle is required.

**Keystore contract:** restart with the same custom `--keystore-path` and password configuration. Password participates in private-key derivation; it is **not keystore-file encryption**. Protect OS permissions and backups separately. Presence RPCs do not prove signing. Proof/signing failure fails generation, not a partial/empty-proof success. Five-key generation is **nontransactional**: unused new entries may remain after failure. Preserve existing keys and fix configuration/error before retrying; never delete a keystore as recovery.

### 5. Coordinator: verify actual source readiness

At one finalized hash compare ordered `Historical.ActiveSessionKeys` / `Session.QueuedKeys` to `Beefy.Authorities` / `Beefy.NextAuthorities`. Require nonempty unique validators and valid non-placeholder unique BEEFY keys, correct `Beefy.SetIdSession`, exact current/next MMR authority IDs/lengths/Merkle commitments and initialized nonzero MMR history.

Source activation allows **1..1000 actual active and queued authorities**, independently of binding, desired `Staking.ValidatorCount` or dormant registrations. Active, queued and **electable standby** operators prepare native keys. An unready electable operator chills through existing staking before selection. There is no readiness election filter, forced chilling or guarantee of future private-key availability. Dormant non-electable owners need not return/purge. No pending-owner counter or custom proof registry gates activation.

Observe actual propagation through normal sessions and an era transition, not a fixed sleep. Registration commonly reaches the session after next, subject to selection/inclusion timing. Preserve archive/handover evidence before pruning. Ordinary purge is neither unbonding nor private-key deletion; coordinate retirement without using it as an activation-counter shortcut.

### 6. Governance: schedule source BEEFY independently

After source readiness/approval, call **`beefy.setNewGenesis(delayInBlocks)`** through approved Root governance, with positive checked future target. **G = execution block + delay**, not proposal block + delay. It is Operational with a separate source **1000-authority reservation**; include real governance-wrapper admission. The argument-aware origin enforces readiness even through Root/bypass dispatch.

No destination binding/cutover is required. Activation/restart changes BEEFY start, not public-chain genesis, MMR, bridge pause/queue/nonce or custody. Verify successful finalized dispatch and `Beefy.GenesisBlock`; after G require advancing cryptographically valid native quorum commitments, handovers and historical MMR proofs across independent nodes. First MMR insertion **A** (upgrade) need not equal BEEFY start **G**.

### 7. Bridge operators: separately pause, drain, bind and enable

Keep legacy GRANDPA traffic operating until its independently approved cutover:

1. **Pause/drain/reconcile:** use the existing approved source/destination freeze and cutoff procedure; stop legacy writers at the reconciled cutoff, inventory roots/messages/consumed nonces/custody and preserve authenticated deliveries. Source pausing does not invalidate already authenticated legacy destination proofs. Enforce destination/application cutoff separately; do not assume source pause drains it or reset source queue/nonce/history.
2. **Verify deployment/capacity:** qualify paired v2 contracts/relay, deployed bytecode, governance and immutable queue/client/chain configuration. Actual current **and queued**, and desired `Staking.ValidatorCount`, must each fit **256**. This is bridge policy, not the source 1000 bound; >256 source BEEFY can activate while bridge traffic remains disabled.
3. **Bind once while paused:** approved Root calls `gearEthBridge.bindDestination(chainId, queue)`. Chain ID is nonzero **32-byte big-endian H256** and queue the approved nonzero 20-byte address. It captures actual nonzero `System.BlockHash(0)` and derives `Keccak256("vara/gear-eth-bridge-domain/v2" || sourceGenesis[32] || chainIdBE[32] || queue[20])`. Verify finalized `DestinationBound`, stored tuple and domain. Binding may happen **before or after BEEFY activation**, only paused/unbound; no rebinding or raw-storage repair.
4. **Authenticate post-binding readiness:** obtain a real signed **post-binding** newest leaf and v2 snapshot under that domain, compare one finalized checkpoint independently, initialize/verify destination client, continue required handovers and accept a later authenticated nonzero root. A pre-binding leaf or domain write is not destination-readiness evidence.
5. **Explicitly unpause:** after destination/replay/custody/canary approval, call `gearEthBridge.unpause` through its authorized governance route and check finalized dispatch. It remains **Normal**, with a separate full-bridge **256-authority allowance**, not source Operational/1000 reservation. Verify matured canary/replay rejection before assets. Maintain Ethereum-to-Gear infrastructure.

Preserve or verifiably migrate consumed-nonce/replay state and custody. Message hashing is unchanged; an empty new queue can replay old consumed messages. Do not run two unrestricted queues for the same assets. MessageQueue has no public verifier setter: a storage-preserving switch or fresh-queue migration needs its reviewed authorized mechanism, not another initialize or invented setter. Recovery must remain reachable when the old verifier expires.

### 8. Later rotations, elections and restarts

Monitor actual active/queued suitability, desired count, native finality, archives, destination freshness and progress. Bound admission fails closed on malformed/missing identity, absent/not-yet-live BEEFY, invalid MMR/descriptors/session mapping or actual/desired >256. Scheduling a later future genesis blocks bound admission **immediately**, including that block. Wait for the new start and verify new signatures/destination readiness. Structural admission cannot guarantee future private signing.

There is **no automatic pause-bit change or queue/destination reset** on readiness failure. Explicitly pause/reconcile when safety requires. Lowering desired count does not shrink oversized actual/queued sets; same-committee sessions can retain them. Do not assume recovery from actual >256: observe a suitable real handover and exact commitments. Per-message admission checks structural/capacity state rather than the full cryptographic scan at unpause.

BEEFY-only rotations preserve the queue; actual GRANDPA changes retain delayed rollover. Pending clear rejects every enqueue path, including governance, with `BridgeCleanupRequired`, preserving queue/nonce. Keep historical root/proof evidence. Existing overflow-reset finalization/GRANDPA-proof rules remain, not activation shortcuts. Readiness rejections preserve fees/message state. Old binaries, repeated scheduling, wiped MMR or fresh queues are not rollback.

## Quorum and destination sampling

Native quorum is **N − floor((N−1)/3)**; **N ≤ 3 requires unanimity**. A process or one signature is not quorum evidence.

Destination claimed native quorum and selected-signature verification are distinct. Existing paired policy uses a **floor(N/3)+1 selection cap and fixed 86/86 floors**, yielding 20 selected at N=59, 51 at 150 and 86 at 256. Confirm Fiat-Shamir/interactive constants in the reviewed artifact; do not use a blanket one-third quorum formula. Interactive RANDAO delay/window remain 128/24 destination blocks.

## Hardware and weight qualification

The [published validator resource baseline](https://wiki.vara.network/docs/vara-network/staking/validate#hardware-requirements) remains **2 vCPUs around 3.4 GHz (Intel Ice Lake or equivalent), 8 GB RAM and at least 80 GB SSD with growth headroom**. Archives need separate sizing. The prebuilt v2.1.0 Linux binaries require **GLIBC 2.39**, as provided by Ubuntu 24.04; the older Ubuntu 22.04 / GLIBC 2.35 wiki baseline does not establish binary compatibility. Verify the approved executable on the actual host before replacing the running node, or use the compatible release container. CPU, memory and storage qualification requirements are unchanged.

Maintainers use a reproducible dedicated host safe for that baseline, recording CPU/frequency/virtualization, RAM/disk/OS, compiler/executor, commit/features, input ranges and repetitions. Ref-time picoseconds and proof bytes are separate; three-second slots have one-second block ref-time budget. Reservations/test exits are not measured baseline execution. Do not tune live-validator CPUs for benchmarks.

| Path | Scope / accounting |
| --- | --- |
| Native registration | Five proofs, effective-controller conversion, invalid proof, repeated full rotation/purge. Retains upstream base set-keys plus **2,500,000,000 ps**; qualify this allowance on the baseline host. |
| Source activation/restart | Independent actual active/queued **1000/1000**, uniqueness, keys, history/MMR commitments; conservative half-Operational-maximum validation reservation plus real wrapper admission. |
| Full bridge unpause | Full **256/256** verification, Normal; upstream unpause plus structural allowance below and a separate **100,000,000,000 ps / 131,072-byte** validation reserve. Normal admission is covered by the runtime budget check. |
| Enqueue/rejection | Bound/legacy last-free-slot, max payload, structural checks and oversize rejection through 1000. Base enqueue plus **500,000,000 ps, 7 MiB proof bytes and 16 DB reads**. Proof reserve covers full declared authority-vector encoding, not only their length prefixes. |
| Migration | All registered owners, active/queued state, ownership/exposure tries; half-block trie headroom **plus size-dependent DB charges**. Neither 256 nor 1000 caps registered-owner migration input. |
| Session/history/MMR | Exposure pages, retention/pruning, immutable activated snapshots, handover and queue/leaf work. Direct V0 trie hashing avoids a single large ABI staging buffer without changing commitments. Measure full transition block; session rotation already charges maximum block weight, not two reservations. |

### Benchmark calibration

Automatic CI compiles benchmark-enabled code but does not run benchmark workloads. The `ci` nextest profile excludes generated benchmark tests while retaining migration and consensus correctness tests. Weight generation and calibration remain explicit manual work on the qualified host; a green CI run is not evidence of validator-baseline performance.

Keep machine-specific timings, test totals, unpublished artifact hashes and raw logs in development evidence, not this operator procedure. Before enactment, promote the required records to the reviewed durable archive and link them from the release record. Local measurements on faster hardware do not qualify the published validator baseline.

The pinned SDK may panic in per-storage proof analysis after collecting native samples (`analysis.rs:286`, empty slopes). A failed benchmark command is not qualification: inspect its log and confirm that every requested execution/verification case and sample completed before considering the raw-input CPU analysis below. Stop on any other error or incomplete data. Raw-input analysis lacks storage metadata and emits zero estimated proof sizes: **never install those generated weights**. Retain declared-storage proof bounds and conservative adapters; qualify CPU and proof budgets independently.

On the qualified host use production profile, **50 steps / 20 repeats**, and a durable evidence directory. Run collection and analysis separately so failures cannot silently flow into later steps:

~~~bash
set -euo pipefail
: "${BENCH_EVIDENCE_DIR:?Set a durable existing evidence directory}"
cargo build -p gear-cli --profile production --locked \
  --features runtime-benchmarks,runtime-benchmarks-checkers
# This SDK CLI requires output files to exist before generation.
touch "$BENCH_EVIDENCE_DIR/native-cpu.rs" "$BENCH_EVIDENCE_DIR/pallet_gear_eth_bridge.rs"
target/production/gear benchmark pallet --chain=dev --steps=50 --repeat=20 \
  --heap-pages=16384 --pallet=beefy_benchmarks --extrinsic='*' \
  --output-analysis=max --output-pov-analysis=max \
  --json-file="$BENCH_EVIDENCE_DIR/native-all.json" > "$BENCH_EVIDENCE_DIR/native-all.log" 2>&1
~~~

Only after checking the collection result and raw-data completeness:

~~~bash
set -euo pipefail
: "${BENCH_EVIDENCE_DIR:?Set the same durable evidence directory}"
# Raw-input CPU analysis avoids the pinned SDK per-storage proof regression bug.
target/production/gear benchmark pallet --json-input="$BENCH_EVIDENCE_DIR/native-all.json" \
  --output-analysis=max --output-pov-analysis=max \
  --output="$BENCH_EVIDENCE_DIR/native-cpu.rs" > "$BENCH_EVIDENCE_DIR/native-analysis.log" 2>&1
target/production/gear benchmark pallet --chain=dev --steps=50 --repeat=20 \
  --heap-pages=16384 --pallet=pallet_gear_eth_bridge --extrinsic='*' \
  --output-analysis=max --output-pov-analysis=max \
  --json-file="$BENCH_EVIDENCE_DIR/bridge-all.json" \
  --output="$BENCH_EVIDENCE_DIR/pallet_gear_eth_bridge.rs" > "$BENCH_EVIDENCE_DIR/bridge-all.log" 2>&1
~~~

These artifacts are nondeployable; `dev` selects benchmark genesis, not fast timing. `activate` independently samples **1..1000 active/queued**; full bound unpause/send **1..256/256**; `bridge_reject_capacity` covers current, next, both and desired oversize through **1000 with full encoded authority values**. Old 256-only source samples are insufficient. Preserve maximum-input observations, rejection state/fee invariants and measured-versus-charged dimensions/margins. FRAME excludes fixture generation/signing/assertions from timing; storage-root work is reported separately.

Measure registration/purge/migration/history/session/MMR and ordinary bridge paths too; extend owner/exposure fixtures for fresh inventory and measure whole session/era hooks. Individual results do not replace exact deployable-WASM staging. Old reference-machine links and Xeon/M4 diagnostics are historical context, not changed hardware requirements or current qualification.

## Evidence status and handover

Keep the operator procedures versioned with the implementation. Development plans, review notes, raw logs and historical local success counts stay outside the repository. They are not release approval. Move required qualification evidence out of temporary storage into the reviewed durable archive before relying on it for a rollout gate.

The coordinator publishes an immutable artifact manifest and appends phase-specific records referencing it:

- Reviewed source commit/tag, review and final-head CI links, workflow run, mainnet node/WASM/metadata checksums, runtime identity and spec/transaction/API versions, feature/timing configuration and actual chain genesis. Mainnet production metadata is built as `vara_runtime_prod.scale`; `vara_runtime.scale` is the testnet metadata.
- Retained mainnet snapshot checksum, finalized block/hash/header root and predecessor identity; matching try-runtime companion checksum; complete migration/decoding/try-state/idempotence/weight results and historical ownership-proof evidence. Reuse the compatible snapshot without changing its identity; a public-testnet snapshot or deployment is not required.
- Baseline-host inventory, benchmark commands/ranges/repetitions, raw samples/logs, measured-versus-charged CPU and proof bounds, live owner/exposure inventory and capacity margin. Include normal-timing exact-deployable-WASM staging, native registration, restart and session/era handover evidence.
- Operator custody/readiness acknowledgements, independent archive endpoints and proof availability, governance approvals and successful finalized inner-dispatch receipts for upgrade and source activation. Append observed propagation, commitments and handovers as they occur.
- For the later bridge cutover: separately reviewed paired revision/deployment identities, destination approvals/bindings, finalized binding/unpause receipts, signed post-binding bootstrap and subsequent accepted roots, legacy drain/replay/custody reconciliation, canary results and durable relay recovery evidence.

Review and CI gate merging; verified artifact identity gates release publication; capacity, exact-artifact staging, current operator readiness and governance gate enactment. Source activation and bridge cutover have separate approvals. Do not mark future receipts complete or require destination approval to publish the node or activate the source. See the [coordinator gates](beefy-migration.md) and [validator procedure](vara/node/README.md#beefy-upgrade-and-later-activation).
