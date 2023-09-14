# Gear block proposer and builder.

This crate provides an extended implementations of the original Substrate's [`BlockBuilder`](https://docs.rs/sc-block-builder/latest/sc_block_builder/struct.BlockBuilder.html) utility and the corresponding runtime api, and a custom implementation of the [`Proposer`](https://docs.rs/sp-consensus/latest/sp_consensus/trait.Proposer.html) trait.

The need for an custom implementations arises from the fact that in Gear a special pseudo-inherent exists that has to be added at the end of each block after all the normal extrinsics have been pushed. Furthermore, to guarantee consistent block production, this pseudo-inherent must not step beyond the block proposing deadline defined by the node client. This requires additional functionality that is not provided by the original `BlockBuilder` utility.

## Overview

For the correct network functioning it is crucial that blocks are created in each time slot.
This requires synchronization between the node client (which operates in terms of milli and nano-seconds) and the `Runtime`
whose notion of time is represented by the block `Weight`.

In the default Substrate's implementation the deadline for the block creation goes through a series of transformations before the proposer actually gets to applying extrinsics to fill the block:

<p align="lift">
    <img src="../../images/block-proposing-timing.svg" width="80%" alt="Gear">
</p>

The original "source of truth" in terms of the block creation time is the same - the `SLOT_DURATOIN` runtime constant. Eventually, the time allocated for processing of extirnsics of all `DispatchClass`'es will constitute almost exactly 1/3 of the overall block time. The same idea is true for the `Runtime` - the `MAXIMUM_BLOCK_WEIGHT` constant calculated as 1/3 of the total possible block `Weight`:

```
`  time, |                        weight |
`    ms  |                               |
`        |                               |
`   3000 |------|                 3*10^12|------|
`        |      |                        |      |
`        |      |                        |      |
`        |      |                        |      |
`        |      |                        |      |
`    980 |------|- proposal        10^12 |------|- max_block
`        |//////|   deadline             |//////|
`        |//////|                        |//////|
`      0 --------------------------------------------
```

As long as this synchronization between how the client counts the remaining time and the `Runtime` that tracks the current block weight (provided the extrinsics benchmarking is adequate) is maintained, we can be almost sure the block timing is consistent: the `DispatchClass::Normal` extrinsics by default are allowed to take up to 80% of the weight, the rest being taken by the inherents, therefore the probability of exhausting the weight before the time has run out (and vise versa) is relatively low.
The finalization of the block is not supposed to be included in this time frame.

## Gear flow specifics

In our case there is a couple of potential pitfalls:
- the share of the `DispatchClass::Normal` extrinsics in the `Runtime` is about 25% as opposed to the default Substrate's 80% (see `NORMAL_DISPATCH_RATIO` constant).
This means that, provided the block proposing deadline is left intact in the client, in case the number of transactions in the `txpool`'s' `ready` queue is high, we can exhaust the allowed block weight for normal extrinsics way before the deadline is reached. It will lead to a situation when extrinsics are "skipped" (marked as "invalid" based on the `check_weight` signed extension) and placed into the `revalidation` queue for the following blocks, while the consumed weight in the current block doesn't change. This could in theory take up to the `soft_deadline` point, which is currently 50% of the whole proposing time. Then, the `Gear::run()` pseudo-inherent is executed before the block finalization, and it assumes it still has at least 3/4 of the block weight to spend. Even if the `Gear::run()` gas consumption matches the expected remaining weight this still may lead to the "Block production took too long" error and the block will be rejected.
<br/>

- The amount of time the `Gear::run()` pseudo-inherent actually takes can also be somewhat indeterministic and depend on lots of variables we can't accurately measure and benchmark upfront. This can result in the same error, as well.

## Custom Proposer implementation

To mitigte the above and keep the block timing consistent a couple of things can be done in our custom block proposer implementation:

- Align the proposing deadline with the block weight corresponding to the normal extrinsics.
There are two ways to go about it:
  1) Manually reduce the "hard" deadline for the extrinsics application proportionally to the respective block weight quota.
  2) Lower the `soft_deadline` percentage to limit the number of tries to add another extrinsic upon the block resources exhaustion.

  Both solutions serve the same purpose - to have a deterministic deadline for the extrinsics application in a block.
  The difference is in nuances.
  Following the first approach, we can set the proposal deadline to, say, `~300ms`, which should be sufficient to use up all the associated weight, at the same time setting the deterministic upper bound on the time interval during which the `Gear::run()` pseudo-inherent starts.
  Following the second approach, we can lower the `soft_deadline` to roughly 20% to ensure that if it turns out we have started skipping extrinsics (exhausted the weight portion), it better be beyond the `soft_deadline` point, so that the maximum attempts to try adding more extrinsics will be limited to the `MAX_SKIPPED_TRANSACTIONS` constant (reduced to 5 from the default 8). This will also set more rigorous bounds on when the `Gear::run()` can start executing. The caveat is, however, that setting an inadeqate `soft_deadline` (too low) can lead to under-populated blocks.

  Current implementation follows the second approach as "less invasive" and more flexible. In case the "Block production took too long" issue persists, switching to the first approach can be considered.
<br/>
- Introduce a timeout for the `Gear::run()` pseudo-inherent to make sure it doesn't spill over the allowed time (in milliseconds). If the timeout is triggered, the pseudo-inherent is dropped and the block is finalized as is (without message queue processing). If this issue persists for a number of blocks, action can be taken offchain to mitigate the problem (like restarting validators with the `--max-gas` option etc.).

## Extended BlockBuilder implementation

In order to serve the purpose, a `BlockBuilder` implementation must be able to:
- Call the Gear specific runtime api to obtain the encoded `Gear::run()` pseudo-inherent from the runtime.
<br/>
- Clone self in order to maintain the underlying runtime api instance with the overlay that contains all the changes made to the state during the extrinsics application and wait for the `Gear::run()` pseudo-inherent completion on the cloned runtime api instance in a separate thread as long as the deadline is not reached. If the deadline has been slipped, the "backup" overlay is used to build the block. Otherwise, the updated overlay returned from the `Gear::run()` thread is used as per normal.

The last point requires quite drastic changes in the original `BlockBuilder` implementation (also to the extent of making changes in the underlying Substrate code in our fork to make some times cloneable, which are not by default). Specifically, we need to be able to clone and destruct the runtime api object into parts and send those parts in a separate thread to restore the `BlockBuilder` in such a way that it has the same state as if it had called the block initialization and all the extrinsics application runtime apis before the `Gear::run()` extrinsic can be applied.
