#![no_std]

use gmeta::metawasm;
use gstd::prelude::*;

#[metawasm]
pub mod metafns {
    pub type State = bool;

    pub fn modified(state: State) -> bool {
        state
    }
}
