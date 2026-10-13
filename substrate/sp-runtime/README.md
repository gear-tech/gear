Runtime Modules shared primitive types.

License: Apache-2.0


## Release

Polkadot SDK stable2409

This local 39.0.5 patch backports typed native session-key generation and required
owner-bound ownership validation. GeneratedSessionKeys contains public keys and
a SCALE signature tuple in field order. Validation consumes the entire proof;
trailing bytes, absent proofs and wrong owners fail. Key generation fails when
any configured keystore key cannot sign; it never returns a partial proof.
