Traits and macros for constructing application specific strongly typed crypto wrappers.

License: Apache-2.0


## Release

Polkadot SDK stable2409

This local 38.0.0 patch backports native session-key possession proofs. Ordinary
sr25519, ed25519 and ECDSA application signatures attest to POP_ || owner;
ECDSA possession rejects invalid scalars and high-S signatures. Experimental
schemes explicitly reject possession generation and verification. Existing
non-possession ECDSA verification remains unchanged.
