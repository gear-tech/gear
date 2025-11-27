## pallet-grandpa-signer

Lightweight pallet to collect GRANDPA signatures for arbitrary payloads. Governance schedules a signing request, validators sign the raw payload with their GRANDPA keys off-chain, and submit an unsigned extrinsic carrying `{request_id, authority_id, signature}`. The pallet verifies membership and signatures against a snapshot of the GRANDPA set, stores signatures until a threshold is reached, and emits events for consumers (e.g., bridges).

### Features
- Unsigned submission validated by the embedded GRANDPA signature (ed25519).
- Scheduling gated by a configurable origin (`ScheduleOrigin`), typically governance/Root, with optional authority subset, threshold, and expiry.
- Snapshot of GRANDPA authorities at scheduling time; rejects submissions from other sets.
- Offchain worker (optional) that can auto-sign pending requests using the local GRANDPA key.
- Pool deduplication and expiry-aware longevity.

### Config
- `AuthorityId`, `AuthoritySignature`: GRANDPA ed25519 types; signatures verified over the raw payload.
- `ScheduleOrigin`: origin allowed to create requests (e.g., `Root` or a governance collective).
- `AuthorityProvider`: source of GRANDPA authorities and `set_id` snapshot.
- `MaxAuthorities`, `MaxPayloadLength`, `MaxRequests`, `MaxSignaturesPerRequest`: storage and validation bounds.
- `UnsignedPriority`: transaction pool priority for submissions.
- `WeightInfo`: weight provider.

### Storage
- `Requests<RequestId -> SigningRequest>`: payload, set_id, authorities, threshold, created_at, expires_at.
- `Signatures<(RequestId, AuthorityId) -> Signature>` and `SignatureCount<RequestId>`.
- `NextRequestId` for incremental IDs.

### Events
- `RequestScheduled { request_id, set_id }`
- `SignatureAdded { request_id, authority, count }`
- `ThresholdReached { request_id }`

### Calls
- `schedule_request(payload, set_id?, authorities?, threshold, expires_at?)` — ScheduleOrigin only.
- `submit_signature(request_id, authority_id, signature)` — unsigned; validated against snapshot and payload.

### Security/DoS considerations
- ValidateUnsigned rejects bad/duplicate/expired submissions and ties longevity to expiry.
- Offchain worker has per-block caps and simple backoff to avoid spamming.
- No fee is charged; spam resistance relies on signature validation and pool deduplication.

### Testing
Unit tests cover scheduling, successful submission, duplicate rejection, expiry, and bad signatures. Run:
```bash
cargo test -p pallet-grandpa-signer --tests
```
