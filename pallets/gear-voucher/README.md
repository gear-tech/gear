# Allows

A crate that implements a flow similar to "payment vouchers". A contract owner (or, actually, anyone, for that matter) may opt for sponsoring users with some amount of funds that can be spent to pay transcation fees and buy gas only for a specific call.

## Interface

### Dispatchable Functions
* `issue` - Issue an `amount` tokens worth voucher for a `user` to be used to pay fees and gas when sending messages to `program`.

License: Unlicensed
