use crate::pallet::{Config, Pallet};

use frame_benchmarking::{benchmarks, whitelisted_caller};
use frame_support::traits::Get;
use gear_core::ids::BuiltinId;
use pallet_gear_builtin_actor::{BuiltinActor, RegisteredBuiltinActor};

impl<T: Config> RegisteredBuiltinActor<Vec<u8>, u64> for Pallet<T> {
    const ID: BuiltinId = BuiltinId(u64::from_le_bytes(*b"bltn/bri"));
}

benchmarks! {
    handle {
        let l in 0 .. T::MaxPayloadLength::get();
        let payload = vec![0; l as usize];
        let builtin_id = Pallet::<T>::ID;
    }: {
        Pallet::<T>::handle(builtin_id, payload);
    }
}
