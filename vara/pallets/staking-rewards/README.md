# Staking Rewards Pallet

The Staking Rewards pallet provides an account to hold a pool of funds that are used to offset
validators' rewards paid at the end of each era so that nominal inflation stays at around zero
for a certain period of time.

Besides, it implements the `pallet_staking::EraPayout` trait to calculate the validators'
rewards and the amount sent to Treasury.

## Overview

The Staking Rewards pallet provides a pool to postpone the inflationary impact of the 
validators rewards until completely depleted after a certain period of time (approx. 2 years).
Thereby the nominal base token inflation stays around zero. Instead, the so-called
"stakeable tokens" amount is increased by the delta minted due to the inflation.
After the pools is depleted the inflation will start affecting the base token total issuance
in a usual Substrate fashion.

## Interface

### Dispatchable Functions

- `set_inflation_parameters` - Update inflation curve parameters.
