Substrate RPC interfaces.

A collection of RPC methods and subscriptions supported by all Substrate clients.

License: GPL-3.0-or-later WITH Classpath-exception-2.0


## Release

Polkadot SDK stable2409

## Gear ChainHead delivery

Each follow subscription queues at most four method-response events, with an 8 MiB
payload budget per event. Oversized storage batches, bodies, and call results return
`operationError`; response sends await capacity outside blocking workers.
The resulting 32 MiB queued-payload bound excludes in-flight operations, backend
allocations, and serialized transport buffers. Existing item truncation and archive
behavior are unchanged.
