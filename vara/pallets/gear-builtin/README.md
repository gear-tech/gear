# Built-in Actor Pallet

The pallet implements a registry of the builtin actors available in the Runtime.

## Overview

Builtin actors claim their unique `BuiltinId`'s that translate into a set of "well-known" accounts (`ActorId`'s), that can be used by other programs to send messages of specific format to perform actions related to blockchain logic implemented in the `Runtime`.

In order for the message queue processor to be able to route messages to the correct destinations (builtin vs. stored programs), there should be a registry implementing the `pallet_gear::BuiltinLookup` trait. This pallet provides the said implementation.

### Dispatchable Functions
The pallet doesn't expose any dispatchable functions callable by the user.
