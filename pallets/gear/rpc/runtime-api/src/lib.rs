#![cfg_attr(not(feature = "std"), no_std)]
#![allow(clippy::too_many_arguments)]
#![allow(clippy::unnecessary_mut_passed)]

use codec::Codec;
use sp_std::vec::Vec;

sp_api::decl_runtime_apis! {
    pub trait GearApi<ProgramId>
    where ProgramId: Codec {
        fn get_gas_spent(program_id: ProgramId, payload: Vec<u8>) -> u64;
    }
}
