use super::*;

impl<T: Config> pallet::Pallet<T> {
    pub fn program_exists(program_id: H256) -> bool {
        common::program_exists(program_id) | Self::paused_program_exists(program_id)
    }
}
