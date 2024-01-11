use crate::{
    pallet::{Config, Pallet},
    IncomingMessage,
};

use frame_benchmarking::{benchmarks, whitelisted_caller};
use frame_support::traits::Get;
use gear_core::ids::BuiltinId;
use pallet_gear_builtin_actor::{BuiltinActor, RegisteredBuiltinActor};
use sp_std::prelude::*;

impl<T: Config> RegisteredBuiltinActor<IncomingMessage, u64> for Pallet<T> {
    const ID: BuiltinId = BuiltinId(u64::from_le_bytes(*b"bltn/bri"));
}

benchmarks! {
    handle {
        let l in 0 .. T::MaxPayloadLength::get();
        let payload = vec![0; l as usize];
        let builtin_id = Pallet::<T>::ID;
        let message = IncomingMessage::new(Default::default(), BuiltinId(0), payload);
    }: {
        Pallet::<T>::handle(&message);
    }
}
