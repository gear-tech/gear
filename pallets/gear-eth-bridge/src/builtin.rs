// This file is part of Gear.

// Copyright (C) 2024-2025 Gear Technologies Inc.
// SPDX-License-Identifier: GPL-3.0-or-later WITH Classpath-exception-2.0

// This program is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.

// This program is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE. See the
// GNU General Public License for more details.

// You should have received a copy of the GNU General Public License
// along with this program. If not, see <https://www.gnu.org/licenses/>.

use crate::{Config, Error, Pallet, TransportFee, WeightInfo};
use common::Origin;
use core::marker::PhantomData;
use gbuiltin_eth_bridge::{Request, Response};
use gear_core::{
    buffer::Payload,
    message::{StoredDispatch, Value},
    str::LimitedStr,
};
use gprimitives::{ActorId, H160};
use pallet_gear_builtin::{BuiltinActor, BuiltinActorError, BuiltinContext, BuiltinReply};
use parity_scale_codec::{Decode, Encode};
use sp_runtime::traits::UniqueSaturatedInto;
use sp_std::vec::Vec;

/// Gear builtin actor providing functionality of `pallet-gear-eth-bridge`.
///
/// Check out `gbuiltin-eth-bridge` to observe builtin interface.
pub struct Actor<T: Config>(PhantomData<T>);

impl<T: Config> BuiltinActor for Actor<T>
where
    T::AccountId: Origin,
{
    fn handle(
        dispatch: &StoredDispatch,
        context: &mut BuiltinContext,
    ) -> Result<BuiltinReply, BuiltinActorError> {
        let source = dispatch.source();

        let from_governance = Pallet::<T>::ensure_admin_or_pauser(source).is_ok();

        let fee: Value = TransportFee::<T>::get().unique_saturated_into();

        if !from_governance && dispatch.value() < fee {
            return Err(BuiltinActorError::InsufficientValue);
        }

        // If the origin is governance, we do not charge a fee and return the full value.
        let refund = if from_governance {
            dispatch.value()
        } else {
            dispatch.value().saturating_sub(fee)
        };

        let request = Request::decode(&mut dispatch.payload_bytes())
            .map_err(|_| BuiltinActorError::DecodingError)?;

        match request {
            Request::SendEthMessage {
                destination,
                payload,
            } => Ok(BuiltinReply {
                payload: send_message_request::<T>(source, destination, payload, context)?,
                value: refund,
            }),
        }
    }

    fn max_gas() -> u64 {
        Default::default()
    }
}

fn send_message_request<T: Config>(
    source: ActorId,
    destination: H160,
    payload: Vec<u8>,
    context: &mut BuiltinContext,
) -> Result<Payload, BuiltinActorError> {
    let gas_cost = <T as Config>::WeightInfo::send_eth_message().ref_time();

    context.try_charge_gas(gas_cost)?;

    Pallet::<T>::queue_message(source, destination, payload)
        .map(|(nonce, hash)| {
            Response::EthMessageQueued { nonce, hash }
                .encode()
                .try_into()
                .unwrap_or_else(|_| unreachable!("response max encoded len is less than maximum"))
        })
        .map_err(|e| BuiltinActorError::Custom(LimitedStr::from_small_str(error_to_str(&e))))
}

pub fn error_to_str<T: Config>(error: &Error<T>) -> &'static str {
    match error {
        Error::BridgeIsNotYetInitialized => "Send message: bridge is not yet initialized",
        Error::BridgeIsPaused => "Send message: bridge is paused",
        Error::MaxPayloadSizeExceeded => "Send message: message max payload size exceeded",
        Error::InsufficientValueApplied => "Send message: insufficient value applied",
        Error::__Ignore(_, _) => unreachable!("never constructed"),
    }
}
