# Test runtime for testing block bulding logic.

This crate a clone of the Substrate's [substrate-test-runtime](https://github.com/paritytech/substrate/tree/master/test-utils/runtime), which defines a signinficantly simplified runtime to test the underlying blockchain machinery rather than any specific runtime features.
The companion crate `gear-test-client` does the same job the [substrate-test-runtime-client](https://github.com/paritytech/substrate/tree/master/test-utils/runtime/client) does - declares and implements a set of traits to facilitate blockchain operations (block creation, initialization, execution, finalization etc.). The Substrate client itslef is provided by the [substrate-test-client](https://github.com/paritytech/substrate/tree/master/test-utils/client) which is generic over runtime abstractions such as Block, Extrinsics, RuntimeApi, as well as low-level Backend, database etc.

The reason the test client wrapper has to be tightly coupled with some runtime is not only because it needs to provide concrete types to the `substrate-runtime-client` but also (and mainly) because it has to offer a way to create a genesis based on a `GenesisConfig` and the runtime compiled wasm code. Since the particular runtime details are not important in the context of blockchain mechanics testing, opting for some trivial runtime looks reasonable.

This, however, has certain limitations owing to the way Gear prodices blocks, namely that a specific unsigned extrinsic needs to be pushed to every block to run queue processing. Therefore an ability to push a custom runtime to the test client can be useful and will likely be introduced in future.

## Interface

License: GPL-3.0.
