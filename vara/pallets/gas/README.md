# Tracking gas creation and consumption

A crate to work with gas. Similarly to how the balances pallets provides tools to mint, burn and transfer
value in a consistent manner, this pallet facilitates tracking of gas creation, usage and consumption
so that ultimately we can prove the system maintains certain economic invariants at all times.

## Interface

### Dispatchable Functions
The pallet doesn't define any extrinsics to be called by external users - only a number of public
functions available in other pallets.

License: Unlicense
