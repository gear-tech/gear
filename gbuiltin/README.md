# gbuiltin crate

A crate that facilitates interaction with Gear runtime builtin actors, that is "actors" that behave like normal programs in Gear implementation of the actor model, but exist as a part of the Runtime rather than a standalone WASM program.

Examples of such builtin actors may include a `staking-proxy` a `governance-proxy` and so on - that is any type that is able to receive an encoded message, do some processing (presumably, implementing some blockchain-specific logic that relies on the chain state) and send an output message to its caller.

This crate defines a sort of a builtin actor communication protocol specification.

This doesn't impose any additional restrictions on builtin actors themselves since there is no way to enforce an actor implemeter to adhere to a specific protocol specification when creating a builtin actor of a specific type. However, the convention is that both actor authors and contracts developers use this protocol spec to avoid ambiguities in communication.
