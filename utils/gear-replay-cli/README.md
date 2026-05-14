# Replaying actual Vara blocks via Remote Externalities

## Overview

In an unlikely event of an error occurring in the `gear node` while processing a block on validators it's often extremely helpful to be able to replay the block with, potentially, more detailed logging level or even with ablitiy to run extrinsics in debugger to quicker understand the root cause of a problem.

Substrate's built-in `try-runtime` machinery largely covers this. The downside of it is that requires one to provide a custom-built Runtime with the `try-runtime` feature enabled so that it had a custom `TryRuntime` runtime api. Also, it lacks Gear-specific runtime features. However, overall the `try-runtime` is a good tool whose functionality goes way beyond the scope of the task in question, especially now that it has been recently moved in a standalone repository [Parity Tech Try Runtime CLI](https://github.com/paritytech/try-runtime-cli) (as opposed to a node subcommand it used to be before).

This crate implements a similar CLI tool allowing to replay blocks on top of both live state and a pre-downloaded snapshot. This tool takes into account the nuances of Gear block production (specifically, the `gear::run` pseudo-inherent that is placed in the end of each block).

Advantages with respect to the Substrate' `try-runtime` are:

- doesn't require enabling additional features;
- can (potentially) still work with the native Runtime implementation for debugging purposes; this, however, would require fiddling with the state machine call to support execution strategies that have been deprecated in Substrate for quite a long time already.

<br/>

## Usage of the `gear-replay-cli` tool

The `gear-replay-cli` tool hides away the complexity of the generic `try-runtime`. Execution strategies are no longer supported (even though might still be re-enabled in the future) which means the runtime that is being used for block execution is either the one from the live chain or the one from the state snapshot. Overriding runtime by using a pre-existing wasm blob (like `try-runtime-cli` can do) is not yet supported.

Note: in the examples below the `--block` argument (either hash or block number) refers to the block we want to replay, therefore the state on top of which the block is applied corresponds to its parent. If not provided, it would be set to the latest finalized head.
This should be kept in mind when creating a snapshot in a separate command.

<br/>

### Examples

General command format and available subcommand are:

```bash
$ gear-replay-cli -h
  Commands of `gear-replay` CLI

  Usage: gear-replay-cli [OPTIONS] <COMMAND>

  Commands:
    replay-block     Replay block subcommand
    gear-run         GearRun subcommand
    create-snapshot  Create a new snapshot file
    help             Print this message or the help of the given subcommand(s)

  Options:
    -l, --log [<NODE_LOG>...]  Sets a custom logging filter. Syntax is `<target>=<level>`, e.g. -lsync=debug
    -h, --help                 Print help (see more with '--help')
```

Currently supported cases include:

- Preparing a snapshot file (which is extremely useful and allows to download large state once and replaying blocks multiple times)
- Replaying a block on a live chain
- Replaying a block on a snapshot

#### Creating state snapshot
Create snapshot command:

```bash
$ gear-replay-cli create-snapshot -h
  Create a new snapshot file

  Usage: gear-replay-cli create-snapshot [OPTIONS] [SNAPSHOT_PATH]

  Arguments:
    [SNAPSHOT_PATH]  The snapshot path to write to

  Options:
    -u, --uri <URI>                    The RPC url [default: wss://archive-rpc.vara.network:443]
    -b, --block <BLOCK>                The block hash or number we want to replay. If omitted, the latest finalized block is used. The blockchain state at previous block with respect to this parameter will be scraped
    -p, --pallet <PALLET>...           Pallet(s) to scrape. Comma-separated multiple items are also accepted. If empty, entire chain state will be scraped
        --prefix <HASHED_PREFIXES>...  Storage entry key prefixes to scrape and inject into the test externalities. Pass as 0x prefixed hex strings. By default, all keys are scraped and included
        --child-tree                   Fetch the child-keys as well
    -h, --help                         Print help (see more with '--help')
```
Usage example:
```bash
gear-replay-cli create-snapshot --uri wss://archive-rpc.vara.network:443 -b 1999999
```

#### Replaying block
Replay block command:
```bash
$ gear-replay-cli replay-block -h
  Replay block subcommand

  Usage: gear-replay-cli replay-block [OPTIONS] <COMMAND>

  Commands:
    snap  Use a state snapshot as the source of runtime state
    live  Use a live chain as the source of runtime state
    help  Print this message or the help of the given subcommand(s)

  Options:
        --block-ws-uri <BLOCK_WS_URI>  The ws uri from which to fetch the block
    -f, --force-run                    Forces `Gear::run()` inherent to be placed in the block
    -h, --help                         Print help (see more with '--help')
```

The `--force` or `-f` option will force the tool to include the `gear::run()` extrinsic in the block even if it had been originally dropped by the block creator (due to panic or time limit violation).

The `--block-ws-uri` is needed in case we use state from local snapshot but still need to download the block itself from somewhere. If the state is `live` this option is ignored.

Usage on live chain:
```bash
gear-replay-cli -lgear,syscalls,pallet replay-block live -u wss://archive-rpc.vara.network:443 -b 0x8dc1e32576c1ad4e28dc141769576efdbc19d0170d427b69edb2261cfc36e905
gear-replay-cli -lgear,syscalls,pallet replay-block --force-run live -u wss://archive-rpc.vara.network:443 -b 2000000
```

Applying block on top of a state snapshot:
```bash
gear-replay-cli -lgear,syscalls,pallet replay-block -f --block-ws-uri wss://archive-rpc.vara.network:443 snap -p ./vara-1200@1999999 -b 2000000
```

Here the state is loaded from the file `./vara-1200@1999999` that is the state corresponding to the previous block with respect the one we want to apply.
<br/>

### Native vs. WASM execution of a block

Since execution strategies have been deprecated in Substrate, it's not possible to use native version of the runtime anymore. However, it seems to be desirable in certain cases in order to introspect and debug issues related to messages processing in Gear.
In theory, we could maintain a custom version of the `sp-state-machine::StateMachine` that'd still accept native runtime for state evolution.
For that purpose we keep the respective artifacts (`NativeElseWasmExecutor` etc.) until we either decide to always stick to wasm runtime only, or have implemented
and tested the necessary parts to switch to native execution. Using `NativeElseWasmExecutor` as opposed to just `WasmExecutor` doesn't do any harm even if the tool is compiled with the `vara-native` feature as it is backward=compatible and will boil down to the `StateMachine::execute()` call which explicitly forces wasm execution.
