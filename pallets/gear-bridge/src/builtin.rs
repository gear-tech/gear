use common::Origin;
use core::marker::PhantomData;
use gbuiltin_bridge::*;
use gear_core::{
    message::{Payload, StoredDispatch},
    str::LimitedStr,
};
use pallet_gear_builtin::{BuiltinActor, BuiltinActorError};
use parity_scale_codec::{Decode, Encode};
use primitive_types::{H160, H256};

use crate::{Config, Error, Pallet};

pub struct Actor<T: Config>(PhantomData<T>);

impl<T: Config> BuiltinActor for Actor<T>
where
    T::AccountId: Origin,
{
    const ID: u64 = 2;

    type Error = BuiltinActorError;

    // TODO (breathx): handle gas limit here.
    fn handle(dispatch: &StoredDispatch, _gas_limit: u64) -> (Result<Payload, Self::Error>, u64) {
        let message = dispatch.message();
        let mut payload = message.payload_bytes();

        let Ok(request) = Request::decode(&mut payload) else {
            return (Err(BuiltinActorError::DecodingError), 0);
        };

        match request {
            Request::SendMessage { dest, payload } => {
                send_message::<T>(dispatch.source().cast(), dest, payload)
            }
        }
    }
}

// TODO (breathx): impl different gas charged.
fn send_message<T: Config>(
    origin: H256,
    dest: H160,
    payload: Payload,
) -> (Result<Payload, BuiltinActorError>, u64)
where
    T::AccountId: Origin,
{
    match Pallet::<T>::send_impl(origin, dest, payload) {
        Ok((nonce, hash)) => {
            let resp = Response::MessageSent { nonce, hash }
                .encode()
                .try_into()
                .unwrap_or_else(|_| unreachable!("Couldn't exceed payload limit due to low size"));
            (Ok(resp), 0)
        }
        // TODO (breathx): impl Display for pallet::Error.
        Err(Error::<T>::BridgePaused) => (
            Err(BuiltinActorError::Custom(LimitedStr::from_small_str(
                "Send message: bridge is paused",
            ))),
            0,
        ),
        Err(Error::<T>::QueueLimitExceeded) => (
            Err(BuiltinActorError::Custom(LimitedStr::from_small_str(
                "Send message: queue is full",
            ))),
            0,
        ),
        _ => (
            Err(BuiltinActorError::Custom(LimitedStr::from_small_str(
                "Send message: unknown",
            ))),
            0,
        ),
    }
}
