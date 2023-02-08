use demo_meta_io::{Id, Person, Wallet};
use gmeta::metawasm;
use gstd::prelude::*;

#[metawasm]
pub mod metafns {
    pub type State = Vec<Wallet>;

    /// Returns a wallet with the given `id`.
    pub fn wallet_by_id(state: State, id: Id) -> Option<Wallet> {
        state.into_iter().find(|w| w.id == id)
    }

    /// Returns a wallet of the given `person`.
    pub fn wallet_by_person(state: State, Person { surname, name }: Person) -> Option<Wallet> {
        let person = Person { surname, name };

        state.into_iter().find(|w| w.person == person)
    }

    /// Returns a wallet of a person with the given `name` & `surname`.
    pub fn wallet_by_name_and_surname(
        state: State,
        name: String,
        surname: String,
    ) -> Option<Wallet> {
        wallet_by_person(state, Person { surname, name })
    }
}
