use demo_meta_io::Wallet;
use gmeta::metawasm;
use gstd::prelude::*;

#[metawasm]
mod functions {
    pub type State = Vec<Wallet>;

    pub fn first_wallet(state: State) -> Option<Wallet> {
        state.first().cloned()
    }

    pub fn last_wallet(state: State) -> Option<Wallet> {
        state.last().cloned()
    }
}
