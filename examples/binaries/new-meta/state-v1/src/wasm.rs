use demo_meta_io::Wallet;
use gmeta::metawasm;
use gstd::prelude::*;

#[metawasm]
pub mod metafuncs {
    pub type State = Vec<Wallet>;

    /// Returns the first wallet.
    pub fn first_wallet(state: State) -> Option<Wallet> {
        state.first().cloned()
    }

    /// Returns the last wallet.
    pub fn last_wallet(state: State) -> Option<Wallet> {
        state.last().cloned()
    }
}
