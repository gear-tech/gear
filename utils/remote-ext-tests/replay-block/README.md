# Replaying actual Vara blocks via Remote Externalities


## Overview

In an unlikely event of an error occurring in the `gear node` while processing a block on validators it's often extremely helpful to be able to replay the block  with, potentially, more detailed logging level or even with ablitiy to run extrinsics in debugger to quicker understand the root cause of a problem.

Substrate's built-in `try-runtime` machinery largely covers this. However, the CLI is a bit cumbersome, and it requires one to provide a custom-built Runtime with the `try-runtime` feature enabled in order for the latter to implement the specific `execute_block` runtime api. Besides, it enforces the `ExecutionStrategy::AlwaysWasm` strategy, that is only allows to use the wasm Runtime which even in case when the `RuntimeVersion` of the native and wasm Runtimes is identical (which should alwasy be the case anyway, to ensure the correct block application).
Regardless of these small inconveniences the `try-runtime` is a good tool whose functionality goes way beyond the scope of the task in question.

In order to mitigate the downsides of the `try-runtime` CLI a dedicated tool has been created. It is dedicated exclusively for remote block execution on top of the on-chain state (currently, only Vara chain is supported).

Advantages with respect to the Substrate' `try-runtime` are:
- doesn't require enabling additional features;
- works with the native Runtime (in case the `RuntimeVersion` is as the one of the on-chain Runtime) rendering debugging possible.

There are also some shortcomings though (which can easliy be fixed in later versions), like the fact it can only download state/block from the live chain and not use a local snapshot.


<br/>

## Substrate's `try-runtime` CLI command

Substrate provides a rather complex machinery for testing various runtime aspects against real live data (via so-called `remote-externalities`).
The funtionality covers blocks execution, rening `on_runtime_upgrade` hooks to test storage migration consistency etc.
It can run against a live chain as well as a downloaded data snapshot.
Runs locally as a node CLI command.

The inconvenience (even, a downside) is that in order to enable the CLI command and the respective runtime api the node and the runtime have to be built with the `try-runtime` feature. It means that even though the actual "live" runtime is downloaded via RPC from an archive node it can't be used for applying extrinsics in a block (because a real runtime would usually not have been built with the `try-runtime` feature on). So in order to use the `try-runtime` command in CLI we need to prepare a runtime of the same version locally (hoping it does, indeed, correspond to the one currently running on-chain).

Other than that it's a good tool.

<br/>

### Usage examples

* Execute block on Vara live chain

    * current latest finalized block on Vara chain

        ```bash
        gear try-runtime --chain=vara --runtime vara_runtime.compact.compressed.wasm execute-block live --uri wss://archive-rpc.vara-network.io:443
        ```

    * at block `$HASH`

        ```bash
        export HASH=0x8dc1e32576c1ad4e28dc141769576efdbc19d0170d427b69edb2261cfc36e905

        gear try-runtime --chain=vara --runtime vara_runtime.compact.compressed.wasm execute-block live --uri wss://archive-rpc.vara-network.io:443 --at "$HASH"
        ```

    *Note:* The `--at` parameter provides the hash of the block which determines the current state of the blockchain. Then the following block is fethced and applied to this state. Therefore if we want to replay extrinsics from the block `N` we must provide the hash of the block `N-1` as the height.


* Execute block against a local snapshot

    * Download snapshot at block `$HASH` (if omitted, the state at the latest finalized block is downloaded)
    
        Note the `existing` value for the `--runtime` option which means the downloaded on-chain runtime is used (the only allowed option for `create-snapshot` command)

        ```bash
        export HASH=0x8dc1e32576c1ad4e28dc141769576efdbc19d0170d427b69edb2261cfc36e905

        gear try-runtime --chain=vara --runtime existing create-snapshot --uri wss://archive-rpc.vara-network.io:443 [--at "$HASH"] [$SNAPSHOT_PATH]
        ```

        If `$SNAPSHOT_PATH` is not provided the default filename for a snapshot would be `$chain`-`$spec_version`@`$block_hash`.snap (for instance, `vara-140@8dc1e32576c1ad4e28dc141769576efdbc19d0170d427b69edb2261cfc36e905.snap` or `vara-140@latest.snap`).

    * Exectute block agains a local snapshot (at block for which the snapshot was created)

        ```bash
        export SNAPSHOT="vara-140@8dc1e32576c1ad4e28dc141769576efdbc19d0170d427b69edb2261cfc36e905.snap"

        gear try-runtime --chain=vara --runtime vara_runtime.compact.compressed.wasm execute-block --block-ws-uri wss://archive-rpc.vara-network.io:443 snap --snapshot-path "$SNAPSHOT"
        ```


        <b>Warning:</b> By default the `try-runtime execute-block` command runs with the `--try-state` option value set to `all`, that is it will try to validate the state of all the pallets in the snapshot. This may result in an error caused by inconsistencies in some pallets storage (for instance, at the moment of this writing the `BagsList` pallet has inconsisent data). This is, of course, something to look into, but it goes beyond the scope of the problem in question.
        Since this doesn't affect the internal logic of the Gear flow we want to reproduce, we might consider either comletely omitting setting the `try_state` by setting the respective option to `none`
        
        ```bash
        gear try-runtime --chain=vara --runtime vara_runtime.compact.compressed.wasm execute-block --try-state none live --uri wss://archive-rpc.vara-network.io:443
        ```
        
        or enumerate a number of pallets in the runtime we are only concerned with:

        ```bash
        export TRY_PALLETS=System,Babe,Grandpa,Balances,Staking,Vesting,Gear,GearGas,GearProgram,GearMessenger,GearScheduler,GearPayment,StakingRewards

        gear try-runtime --chain=vara --runtime vara_runtime.compact.compressed.wasm execute-block --try-state "$TRY_PALLETS" live --uri wss://archive-rpc.vara-network.io:443
        ```


<br/>

## Replaying block with `remote-ext-tests-replay-block` custom CLI

The `remote-ext-tests-replay-block` CLI hides away the complexity of the generic `try-runtime`. The execution strategy used to apply a block is `ExecutionStrategy::NativeElseWasm`, which in case of the correct `RuntimeVersion` will allow using the native implementation of the `Core` runtime api.

Only live chain state (via remote externalities) is currently supported.

Another difference from the `try-runtime` API is that in `remote-ext-tests-replay-block` CLI the block (hash or number) provided as the `--block` argument is the one whose extrinsics we want to apply. It means the blockchain state we scrape from the live chain would correspond to the previous block with respect to the one provided. If the `--block` argument is omitted the last finalized block from the live chain is used.

<br/>

### Usage examples

In order to use the native runtime build make sure the node is built with the Runtime spec version that matches the one currently uploaded on chain.

* Replay a block on Vara live chain

    * current latest finalized block on Vara chain

        ```bash
        export RUST_LOG=remote-ext::cli=info,gear::runtime=debug

        remote-ext-tests-replay-block --uri wss://archive-rpc.vara-network.io:443
        ```

    * block with `$HASH` or `$BLOCK_NUM`

        ```bash
        export HASH=0x8dc1e32576c1ad4e28dc141769576efdbc19d0170d427b69edb2261cfc36e905
        export BLOCK_NUM=2000000

        remote-ext-tests-replay-block --uri wss://archive-rpc.vara-network.io:443 --block "$HASH"
        remote-ext-tests-replay-block --uri wss://archive-rpc.vara-network.io:443 --block "$BLOCK_NUM"
        ```

<br/>

### Native vs. WASM execution of a block

The `remote-ext-tests-replay-block` CLI tools provides means to enable execution of a downloaded block both against the on-chain WASM Runtime as well as the native local Runtime, provided the version of the latter matches the on-chain Runtime version. This can be useful for debugging.

The `wasm-only` version which runs the downloaded Runtime is lighter-weight (the executable is about 40% smaller as it doesn't include the `vara`- or `gear-runtime` as a dependency), and it works with both Gear testnet and the Vara chain (provided the user supplies the correct WebSocker connection uri).
This is the default way of building the tool:    
```bash
./scripts/gear.sh build remote-ext-tests --release
```
or simply
    
```bash
make remote-ext-tests
```

In order to enable native runtime, use one of the following:
```bash
make remote-ext-tests-vara-native
make remote-ext-tests-gear-native
```

or

```bash
./scripts/gear.sh build remote-ext-tests --release --no-default-features --features=vara-native
./scripts/gear.sh build remote-ext-tests --release --no-default-features --features=gear-native
```

The `--uri` parameter must match the native runtime you've built the tool with, while the Runtime version should be the same as the on-chain one. If it does not, the Wasm executor will be used as a fallback.
