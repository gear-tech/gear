# Built-in Actor Pallet

The pallet provides a built-in actor (account) directly in the `Vara runtime` that would allow to build applications on top of some runtime-related logic like `Staking`, `Governance` an so on. Other actor in the system (a.k.a. `programs`) can interact with the built-in actor by sending messages to this actor and receiving its replies.

The idea behind this pallet is to avoid introducing additional system calls supported by the environment thereby keeping the wasm processing machinery as lean as possible.

## Overview

The Built-in actor defines a set of unique "well-known" accounts that, when converted into `ProgramId`'s, can be used by other programs to send messages of specific format to perform actions related to blockchain logic implemented in the `Runtime`.
At the time a message is popped from the queue, in case the destination matches one of the built-in addresses, the built-in actor's `handle()` method is invoked directly skipping the complex message-processing pipeline. The built-in actor then decodes a message, wraps it in a form of dispatchable call and relays it to another runtime pallet (`Staking` etc.) on behalf of the contract the had sent the original message.

The `handle()` method produces a set of outcomes (`JournalNote`'s) that are then processed by the `core-processor` in order to tally the consumed gas and create and send the reply message.

In terms of the cost (or consumed gas), the proxied messages, like ordinary messages processed by the wasm executor, are not free of charge and must be paid for. The amount of gas required to process a message is determined by the actual weight of the underlying dispatchable call in the respective pallet (`Staking` or what not).

In the message doesn't have enough gas allocated to it no dispatch take place and the reply message with an error code is sent back straight away.

### Dispatchable Functions
The pallet doesn't expose any dispatchable functions callable by the user. An attempt to send a user message to the built-in actor will result in a runtime error.
