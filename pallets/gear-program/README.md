# Tracking and charging fee for on-chain resources usage

A module providing extrinsics and offchain workers for things like charging rent for keeping messages in the wait list etc.

## Interface

### Dispatchable Functions

* `collect_waitlist_rent` - called by any external account to collect wait list rent for a set of message IDs

License: Unlicense
