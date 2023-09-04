use demo_meta_io::Wallet;
use gmeta::metawasm;
use gstd::prelude::*;

#[metawasm]
pub mod metafns {
    pub type State = Vec<Wallet>;

    /// Returns the first wallet.
    pub fn first_wallet(state: State) -> Option<Wallet> {
        state.first().cloned()
    }

    /// Returns the last wallet.
    pub fn last_wallet(state: State) -> Option<Wallet> {
        state.last().cloned()
    }

    /// Returns the first & last wallets.
    ///
    /// They'll equal if the contract has only one wallet.
    pub fn first_and_last_wallets(state: State) -> (Option<Wallet>, Option<Wallet>) {
        (first_wallet(state.clone()), last_wallet(state))
    }
}
