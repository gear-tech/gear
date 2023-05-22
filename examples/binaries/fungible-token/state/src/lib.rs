#![no_std]

use ft_io::*;
use gmeta::{metawasm, Metadata};
use gstd::{prelude::*, ActorId};

#[metawasm]
pub trait Metawasm {
    type State = <FungibleTokenMetadata as Metadata>::State;

    fn name(state: Self::State) -> String {
        state.name.clone()
    }
    fn symbol(state: Self::State) -> String {
        state.symbol.clone()
    }
    fn decimals(state: Self::State) -> u8 {
        state.decimals
    }
    fn total_supply(state: Self::State) -> u128 {
        state.total_supply
    }

    fn balances_of(account: ActorId, state: Self::State) -> u128 {
        match state.balances.iter().find(|(id, _balance)| account.eq(id)) {
            Some((_id, balance)) => *balance,
            None => panic!("Balance for account ID {account:?} not found",),
        }
    }
}
