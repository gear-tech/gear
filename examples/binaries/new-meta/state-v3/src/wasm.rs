use demo_meta_io::Wallet;
use gmeta::metawasm;
use gstd::{exec, prelude::*};

#[metawasm]
pub mod metafuncs {
    pub type State = Vec<Wallet>;

    pub fn block_number(state: State) -> u32 {
        // TODO: allow state to be unused
        let _ = state;

        exec::block_height()
    }

    pub fn block_timestamp(state: State) -> u64 {
        // TODO: allow state to be unused
        let _ = state;

        exec::block_timestamp()
    }
}
