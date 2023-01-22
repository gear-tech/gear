use demo_meta_io::{Id, Person, Wallet};
use gmeta::metawasm;
use gstd::prelude::*;

#[metawasm]
mod functions {
    pub type State = Vec<Wallet>;

    pub fn wallet_by_id(state: State, id: Id) -> Option<Wallet> {
        state.into_iter().find(|w| w.id == id)
    }

    pub fn wallet_by_person(state: State, person: Person) -> Option<Wallet> {
        state.into_iter().find(|w| w.person == person)
    }
}
