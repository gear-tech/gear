use demo_meta_io::{Id, Person, Wallet};
use gmeta::metawasm;
use gstd::prelude::*;

#[metawasm]
pub mod metafuncs {
    pub type State = Vec<Wallet>;

    /// Returns a wallet with the given `id`.
    pub fn wallet_by_id(state: State, id: Id) -> Option<Wallet> {
        state.into_iter().find(|w| w.id == id)
    }

    /// Returns a wallet of the given `person`.
    pub fn wallet_by_person(state: State, person: Person) -> Option<Wallet> {
        state.into_iter().find(|w| w.person == person)
    }
}
