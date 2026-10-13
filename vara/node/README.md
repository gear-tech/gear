# Gear Node

Gear Substrate-based node, ready for hacking.

Gear node is a key element of the Vara blockchain network. In a nutshell, it is a standard Substrate node with many low-level modules being used out-of-the-box, specifically, the consensus layer, libp2p networking etc. There are some modifications though, which cater to the specific needs of the Gear runtime as a platform for Wasm-based dApps. The most notable one is a custom block authorship logic brought to ensure that the main invariants the Gear protocol relies on, are upheld:
- the messages queue is processed last in a block and the processing has enough time to run;
- there is always a block within a slot, regardless of potentially indeterministic behavior of the programs execution.

## Building from source

### 1. Install dependencies

#### Ubuntu/Debian
```
sudo apt update
# May prompt for location information
sudo apt install -y git clang curl libssl-dev llvm libudev-dev cmake protobuf-compiler
```

#### MacOS
```
# Install Homebrew if necessary https://brew.sh/
/bin/bash -c "$(curl -fsSL https://raw.githubusercontent.com/Homebrew/install/master/install.sh)"

# Make sure Homebrew is up-to-date, install openssl
brew update
brew install openssl
```

Additionally, if you use Apple Silicon (M1/M1 Pro/M1 Max), install Rosetta:
```
/usr/sbin/softwareupdate --install-rosetta --agree-to-license
```

#### Windows

Windows 10 is supported with WSL!

- Install WSL and upgrade it to version 2 use instructions from https://docs.microsoft.com/en-us/windows/wsl/install-win10.
- Ensure VM feature is enabled in bios in processor advanced menu.
- Install Ubuntu 20.04 LTS https://www.microsoft.com/store/apps/9n6svws3rx71.
- Launch installed app and setup root user - exit ubuntu app (first time launch takes time).
- Install windows terminal from app store or use VSCode with remote plugin (auto suggested once wsl is detected by VSCode).
- Follow instructions for linux.

### 2. Rust and all toolchains

If Rust is not yet installed, read the [Installation](https://doc.rust-lang.org/book/ch01-01-installation.html) part from [The Book](https://doc.rust-lang.org/book/index.html) to install it.

Make sure the `wasm` target is enabled:
```bash
rustup target add wasm32v1-none
```

Set the environment variables:
```
source ~/.cargo/env
```

### 3. Build the node

Run the following commands to build the node:
```bash
make node-release
```

The resulting binary will be located at `./target/release/gear`.

## Running a dev node

To run a local dev network, execute the following command:

  ```bash
  gear --dev
  ```

By providing an additional argument one can specify the location of the chain database:
  
  ```bash
  gear --dev --base-path /tmp/vara
  ```

Now the dev node is listening on the [default] rpc port 9944: https://polkadot.js.org/apps/?rpc=ws%3A%2F%2F127.0.0.1%3A9944

The list of available subcommands and command-line options can be obtained by running:

  ```bash
  gear --help

  Usage: gear [OPTIONS]
        gear <COMMAND>

  Commands:
    key            Key management cli utilities
    build-spec     Build a chain specification
    check-block    Validate blocks
    export-blocks  Export blocks
    export-state   Export the state of a given block into a chain spec
    import-blocks  Import blocks
    purge-chain    Remove the whole chain
    revert         Revert the chain to a previous state
    try-runtime    Try-runtime has migrated to a standalone CLI (<https://github.com/paritytech/try-runtime-cli>). The subcommand exists as a stub and deprecation notice. It will be removed entirely some time after January 2024
    chain-info     Db meta columns information
    help           Print this message or the help of the given subcommand(s)

  Options:
    ...
  ```
For instance, complete clean-up of a chain state and blockstore can be done by purging the chain:

  ```bash
  gear purge-chain --dev
  ```

## More advanced modes

### Multi-node local Vara network

Running a local testnet with two validator nodes - Alice and Bob, allows to watch the multi-node consensus algorithm in action.
Note that if you launch both nodes on the same machine, you need to specify different ports for each node.

Start the `alice` node first:

  ```bash
  gear --alice --chain=local --base-path ./tmp/alice --port 30333 --rpc-port 9944 --validator
  ```

While the node is starting, inspect the start up log and look for the line that would look like the one below:

  ```bash
  2024-01-01 11:23:05 🏷  Local node identity is: 12D3KooWMar4rG4kfoCZA1sqaY8FqtPDgpBPfDnQ7Md9x6Sdkgw5
  ```
Take note of the node identity string.

Now open another terminal window and start the `bob` node. Note that since both nodes are going to be running on the same machine we should choose different tcp and ws ports (for libp2p and rpc connections) for each node.
Also, we need to specify the `--bootnodes` parameter by providing the multiaddress of the `alice` node to let `bob` know where to look for its peer:

  ```bash
  gear \
    --bob \
    --chain=local \
    --base-path ./tmp/bob \
    --port 30334 \
    --rpc-port 9945 \
    --bootnodes /ip4/127.0.0.1/tcp/30333/p2p/12D3KooWMar4rG4kfoCZA1sqaY8FqtPDgpBPfDnQ7Md9x6Sdkgw5
    --validator
  ```

Having done this, you should see the `bob` node connecting to the `alice` node and starting to produce blocks.

Check the network status at https://polkadot.js.org/apps/?rpc=ws%3A%2F%2F127.0.0.1%3A9944.

### Connect to the Vara mainnet

Running a node that would sync with the Vara mainnet is as simple as running the following command:

  ```bash
  gear --chain=vara
  ```

As before, supplying a variety of CLI arguments allows to customize your node in terms of the chain database location, rpc port, and so on.

### Running an archive node

In some projects it can be useful to store all historical data. To run an archive node, use the following command:

  ```bash
  gear --chain=vara --blocks-pruning=archive --state-pruning=512
  ```
where the `--state-pruning` value specifies the history depth (in terms of the number of blocks) of the state to be kept in the database. All other CLI options apply, as usual.

Turning on the archiving option will significantly increase the disk space usage as well as impact the node's performance. This should be done judiciously.

### Connect to Vara testnet

Finally, calling simply
  
  ```bash
  gear
  ```
will connect you to the default chain, which is Vara testnet.

### Connect to a custom chain

To connect to a custom chain, the first thing one needs to do is to obtain the chain specification JSON file. Then calling the following command will start a node which will then try to connect to the bootnodes from the provided chain specification and start syncing blocks:

  ```bash
  gear --chain=/path/to/your/chain/spec.json
  ```

### Run the node as validator

To run a Vara network validator, start the node with `--validator` and complete the bonding/session registration described in the [Vara validator guide](https://wiki.vara.network/docs/vara-network/staking/validate). For the five-key BEEFY upgrade, use the migration procedure below instead of the wiki's legacy empty-proof registration flow.

### BEEFY upgrade and later activation

For the mainnet rollout follow the [coordinator checklist](../../beefy-migration.md#mainnet-coordinator-checklist) and [mainnet operator runbook](../../beefy-deployment.md#mainnet-operator-runbook). **Node installation, runtime upgrade, source BEEFY activation and Ethereum bridge cutover are four separate actions.** Your job is to install the published node, preserve custody, register native keys after the upgrade, and report actual duty readiness. You do not build runtime WASM, run maintainer benchmarks or execute governance actions.

#### Before the runtime upgrade

- [ ] Obtain the coordinator's pinned mainnet release tag/manifest; verify the published node checksum. Before stopping the old service, execute the downloaded node with `--version` on this host: the prebuilt v2.1.0 Linux binary requires **GLIBC 2.39 / Ubuntu 24.04 or a compatible release container**, not Debian 12 / Ubuntu 22.04. Keep **`--chain vara`** explicitly: starting `gear` without a chain selects testnet.
- [ ] Back up/protect the existing service configuration, database/base path, network identity, complete keystore and password configuration. Never send seeds, private keys, keystore files or passwords to the coordinator.
- [ ] In your assigned window, stop the old process, install the approved binary and restart the same service/configuration. Never purge/resync or run two signers with the same keys. Keep unsafe authoring RPC loopback-only/access controlled, validator mode and consensus networking enabled.
- [ ] Verify mainnet identity, peer connectivity, advancing finalized blocks and current validator duties after restart. Send the pre-upgrade acknowledgement; do not rotate keys until the coordinator confirms finalized API v2.

**Install/restart the new node BEFORE upgrading the runtime.** Old nodes are incompatible with the changed SessionKeys API v2 ABI. The new node's API v1 fallback only lets it run the four-key predecessor before upgrade; it does not make old nodes compatible with the new runtime. Do not register five-key bundles/proofs on the predecessor.

Enable `--enable-offchain-indexing true` before first MMR insertion at runtime upgrade. Independent proof servers use `--state-pruning archive --blocks-pruning archive` and need actual historical proof availability from that insertion; ordinary validators need not all become archives. Flags cannot recover pruned state or backfill missing offchain MMR nodes. Late servers require verified replay/recovery.

#### Upgrade and native ownership

The exact migration is **11000 → 20100**, preserving `vara` / `vara-testnet` identity, four public-key fields, ordered active/queued sets and key ownership. It adds deterministic `0x02 || Keccak256(stash.raw32)` placeholder BEEFY keys, preserves bridge pause state/queue/nonce/owners/history, and leaves BEEFY inactive. Historical roots remain available under the migration's legacy-root retention policy. No bridge pause, binding or reset happens automatically.

After finalized enactment, check code/checksum, metadata and `state_getRuntimeVersion(finalizedHash)` for spec 20100 and SessionKeys API **2**, ID **`0xab3c0572291feb8b`** (pinned SDK Blake2b-64 hash of `SessionKeys`). Use the runbook's curl examples and approved metadata-aware queries to read `Staking.Bonded(stash)` and resolve the effective signed owner: controller where applicable or represented proxy/multisig account, not the outer fee payer.

Generate on the released local node using the runbook's [native-generation example](../../beefy-deployment.md#4-validators-native-generation-and-ordinary-registration).
Set `OWNER` to the actual effective signer's **raw 32-byte AccountId32 hex**, not SS58 text or a length-prefixed SCALE Vec.

Use your service's actual local RPC port. Require `result.keys` **161 bytes** in BABE/GRANDPA/ImOnline/AuthorityDiscovery/BEEFY order and nonempty `result.proof` **321 bytes**, the native five-signature tuple. Every key signs `POP_ || owner`; ECDSA possession uses normal Blake2-based signatures, not prehashed BEEFY commitment signatures, and rejects noncanonical high-S. This protocol is owner-bound, not genesis-/whole-bundle-bound. Never use `author_rotateKeys` with empty proof, manually concatenate keys or export session secrets.

Production native generation uses **seed None** and rotates **all five keys**, not only BEEFY. Back up/protect the entire keystore and retain every old private entry until active/queued duties and offence-proof/recovery retention end. Custom `--keystore-path` and password configuration must match on restart. Password participates in derivation, **not file encryption**; secure filesystem and backups separately. Generation/proof signing failure is an RPC failure, not partial/empty-proof success; generation is nontransactional and unused keys may remain after failure. Fix configuration/errors without deleting existing keys. Presence RPCs are not proof of signing.

- [ ] With approved ordinary metadata-aware transaction tooling submit **`session.setKeys(keys, proof)`** from the effective account, funded with liquid unbonded fee balance. Use a generic extrinsic interface if a wizard still sends empty proof.
- [ ] Require finalized successful inner dispatch and exact `Session.NextKeys(stash)` equality; a transaction hash is not success. No separate signer, code download or secret file is required.
- [ ] Send the public registration acknowledgement below. Keep old keys and continue existing duties until actual queued/active propagation is observed; do not assume a fixed two-session sleep proves readiness.

#### Acknowledgements to the coordinator

Report public evidence at each phase, not secrets:

| Phase | Send |
| --- | --- |
| Before runtime enactment | Stash and effective signing account; approved release tag/node version/binary SHA-256; mainnet genesis and observed finalized block/hash; confirmation that the same service/database/keystore/password is in use, backups are protected and offchain indexing is enabled. Designated archives also confirm state/block retention and proof-service endpoints. |
| After native registration | Finalized `vara/20100` block/hash; API v2 confirmation; effective owner as raw AccountId32; public `keys` and `proof`; registration extrinsic hash, finalized block and successful inner dispatch; exact `NextKeys(stash)` match. |
| After propagation/activation | Observed queued and active session IDs with finalized block/hash; continued BABE/GRANDPA duties; BEEFY public key and evidence of native signing/commitment progress when active; confirmation that old keys remain retained. Let the coordinator reconcile all reports at common finalized state. |

If an acknowledgement cannot be supplied, report the specific error immediately and do not improvise a purge, empty proof, raw storage edit or genesis reset. An electable operator unable to prepare native keys must coordinate chilling before selection; this does not authorize stopping an active signer prematurely.

#### Independent source activation

Active, queued and **electable standby** operators prepare native keys; unready electable operators chill using existing staking before election. Dormant non-electable owners need not return/purge. There is no readiness election filter, forced chilling, pending-owner counter or persistent custom proof ledger. A registration proof cannot guarantee future private-key availability.

Source readiness checks **1..1000 actual active/queued** authorities, independent of binding and desired `Staking.ValidatorCount`: valid unique non-placeholder keys and validators, exact ordered BEEFY lists, session mapping and current/next MMR IDs/lengths/roots, nonzero initialized history. Observe actual propagation across normal sessions/era, not a fixed sleep or key-presence response.

After independent source approval, governance calls `beefy.setNewGenesis(delayInBlocks > 0)` for a checked future execution-block-plus-delay target. It is Operational with a separate source-1000 reservation and readiness enforced by the argument-aware origin, including bypass dispatch. No destination binding is required. It changes BEEFY start, not public-chain genesis, MMR or bridge pause/queue/nonce/custody. Observe real cryptographically valid native commitments and handovers across independent nodes. Native quorum is **N − floor((N−1)/3)**, unanimous at **N ≤ 3**. First MMR insertion A and BEEFY start G need not coincide.

#### Separately approved bridge cutover

Bridge cutover is the coordinator's separately approved operation, not a validator registration step. Continue the assigned consensus duties and retain old keys, historical proofs and archives. Do not independently pause/unpause the bridge, bind a destination, purge keys or schedule a BEEFY restart to repair readiness. Report finality, signing and proof-service failures immediately.

The [coordinator gates](../../beefy-migration.md#gate-3-preserve-legacy-delivery-custody-and-replay-state) cover legacy delivery, destination verification, replay/custody reconciliation and explicit unpause. Source activation alone does not authorize bridge traffic or asset movement.

#### Release and qualification status

Mainnet uses published `production_vara_runtime_v*.wasm` and its matching production metadata, never the `testnet_vara_runtime_v*.wasm` artifact. The node's embedded testnet runtime is expected, but retain `--chain vara`. The coordinator verifies the mainnet checksums/manifest, normal timing, supported-state rehearsal and exact-artifact qualification; validators install the published node only. Public-testnet deployment is not a prerequisite for this rollout.

The [published validator baseline](https://wiki.vara.network/docs/vara-network/staking/validate#hardware-requirements) stays **2 vCPUs ~3.4 GHz (Ice Lake or equivalent), 8 GB RAM, Ubuntu 22.04+ / GLIBC 2.35+, at least 80 GB SSD with headroom**; archives need separate sizing. No new BEEFY hardware minimum is introduced. Maintainers qualify baseline-host migration/session/MMR/native-registration/source-1000/full-bridge-256 and 1000-authority rejection costs; validators do not benchmark live machines.

Maintainer [weight qualification](../../beefy-deployment.md#hardware-and-weight-qualification) and the [durable release record](../../beefy-deployment.md#evidence-status-and-handover) define the required evidence. Follow the coordinator's pinned approved manifest, not local benchmark hashes or historical test totals. CI checks alone do not authorize publication or network enactment.

### Offline BEEFY proof recovery

With the source database stopped and a fresh destination database, `export-blocks` / `import-blocks` preserve authenticated BEEFY proofs even when the exported GRANDPA checkpoints end earlier. The import checks signatures against each imported block's runtime state and retains file proofs separately; it does not advance GRANDPA finality or BEEFY voting state. Live proof serving and recovery wait for canonical GRANDPA finality.

```bash
gear export-blocks --chain vara --base-path /path/to/source blocks.json
gear import-blocks --chain vara --base-path /path/to/recovery \
  --state-pruning archive --blocks-pruning archive blocks.json
```

Use a fresh archive database for `/path/to/recovery`, and keep the original block file until a re-export has verified its BEEFY proofs. Block import and proof archival are separate commits. If an import fails or is interrupted, replay the complete original file into another fresh database: repeating it against the partial database skips already-known blocks and cannot repair missing proofs. Do not replace the old database or remove backups until recovery verification is complete.

The file-proof archive remains in the database's auxiliary storage; block pruning does not remove those copies. Size storage for the imported proof history. Block exports are not full database or keystore backups and do not contain offchain MMR nodes. Keep existing backups and require verified MMR replay/recovery before advertising a restored proof service.
