use demo_meta_io::Wallet;
use gmeta::metawasm;
use gstd::{exec, prelude::*};

#[metawasm]
pub mod metafns {
    pub type State = Vec<Wallet>;

    pub fn block_number(_: State) -> u32 {
        exec::block_height()
    }

    pub fn block_timestamp(_: State) -> u64 {
        exec::block_timestamp()
    }
}
