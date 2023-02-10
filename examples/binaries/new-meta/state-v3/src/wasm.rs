use demo_meta_io::Wallet;
use gmeta::metawasm;
use gstd::{exec, prelude::*};

#[metawasm]
pub trait Metawasm {
    type State = Vec<Wallet>;

    fn block_number(state: Self::State) -> u32 {
        // TODO: allow state to be unused
        let _ = state;

        exec::block_height()
    }

    fn block_timestamp(state: Self::State) -> u64 {
        // TODO: allow state to be unused
        let _ = state;

        exec::block_timestamp()
    }
}
