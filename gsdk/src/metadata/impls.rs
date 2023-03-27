// This file is part of Gear.
//
// Copyright (C) 2021-2022 Gear Technologies Inc.
// SPDX-License-Identifier: GPL-3.0-or-later WITH Classpath-exception-2.0
//
// This program is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.
//
// This program is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE. See the
// GNU General Public License for more details.
//
// You should have received a copy of the GNU General Public License
// along with this program. If not, see <https://www.gnu.org/licenses/>.

// Copyright (C) 2022 Gear Technologies Inc.
// SPDX-License-Identifier: GPL-3.0-or-later WITH Classpath-exception-2.0
//
// This program is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.
//
// This program is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE. See the
// GNU General Public License for more details.
//
// You should have received a copy of the GNU General Public License
// along with this program. If not, see <https://www.gnu.org/licenses/>.

use super::runtime_types::{
    frame_system::pallet::Call as SystemCall,
    gear_common::{event::*, gas_provider::node::GasNodeId},
    gear_core::{ids as generated_ids, message as generated_message},
    gear_runtime::{RuntimeCall, RuntimeEvent},
    pallet_balances::pallet::Call as BalancesCall,
    pallet_gear::pallet::Call as GearCall,
    pallet_sudo::pallet::Call as SudoCall,
};
use gear_core::{ids, message, message::StoredMessage};
use parity_scale_codec::{Decode, Encode};
use sp_runtime::MultiAddress;
use subxt::dynamic::Value;

type ApiEvent = super::Event;

impl From<ids::MessageId> for generated_ids::MessageId {
    fn from(other: ids::MessageId) -> Self {
        Self(other.into())
    }
}

impl From<generated_ids::MessageId> for ids::MessageId {
    fn from(other: generated_ids::MessageId) -> Self {
        other.0.into()
    }
}

impl From<ids::ProgramId> for generated_ids::ProgramId {
    fn from(other: ids::ProgramId) -> Self {
        Self(other.into())
    }
}

impl From<generated_ids::ProgramId> for ids::ProgramId {
    fn from(other: generated_ids::ProgramId) -> Self {
        other.0.into()
    }
}

impl From<ids::CodeId> for generated_ids::CodeId {
    fn from(other: ids::CodeId) -> Self {
        Self(other.into())
    }
}

impl From<generated_ids::CodeId> for ids::CodeId {
    fn from(other: generated_ids::CodeId) -> Self {
        other.0.into()
    }
}

impl From<generated_ids::ReservationId> for ids::ReservationId {
    fn from(other: generated_ids::ReservationId) -> Self {
        other.0.into()
    }
}

impl From<generated_message::common::ReplyDetails> for message::ReplyDetails {
    fn from(other: generated_message::common::ReplyDetails) -> Self {
        message::ReplyDetails::new(other.reply_to.into(), other.status_code)
    }
}

impl From<generated_message::common::SignalDetails> for message::SignalDetails {
    fn from(other: generated_message::common::SignalDetails) -> Self {
        message::SignalDetails::new(other.from.into(), other.status_code)
    }
}

impl From<generated_message::common::MessageDetails> for message::MessageDetails {
    fn from(other: generated_message::common::MessageDetails) -> Self {
        match other {
            generated_message::common::MessageDetails::Reply(reply) => Self::Reply(reply.into()),
            generated_message::common::MessageDetails::Signal(signal) => {
                Self::Signal(signal.into())
            }
        }
    }
}

impl From<generated_message::stored::StoredMessage> for message::StoredMessage {
    fn from(other: generated_message::stored::StoredMessage) -> Self {
        StoredMessage::new(
            other.id.into(),
            other.source.into(),
            other.destination.into(),
            // converting data from the same type
            other.payload.0.try_into().expect("Infallible"),
            other.value,
            other.details.map(Into::into),
        )
    }
}

impl From<ApiEvent> for RuntimeEvent {
    fn from(ev: ApiEvent) -> Self {
        RuntimeEvent::decode(&mut ev.encode().as_ref()).expect("Infallible")
    }
}

impl From<RuntimeEvent> for ApiEvent {
    fn from(ev: RuntimeEvent) -> Self {
        ApiEvent::decode(&mut ev.encode().as_ref()).expect("Infallible")
    }
}

impl<M> From<generated_ids::ReservationId> for GasNodeId<M, ids::ReservationId> {
    fn from(other: generated_ids::ReservationId) -> Self {
        GasNodeId::Reservation(other.into())
    }
}

impl<M: Clone, R: Clone> Clone for GasNodeId<M, R> {
    fn clone(&self) -> Self {
        match self {
            GasNodeId::Node(message_id) => GasNodeId::Node(message_id.clone()),
            GasNodeId::Reservation(reservation_id) => {
                GasNodeId::Reservation(reservation_id.clone())
            }
        }
    }
}

impl<M: Copy, R: Copy> Copy for GasNodeId<M, R> {}

macro_rules! impl_basic {
    ($t:ty) => {
        impl Clone for $t {
            fn clone(&self) -> Self {
                Self::decode(&mut self.encode().as_ref()).expect("Infallible")
            }
        }

        impl PartialEq for $t {
            fn eq(&self, other: &Self) -> bool {
                self.encode().eq(&other.encode())
            }
        }
    };
    ($t:ty $(, $tt:ty) +) => {
        impl_basic!{ $t }
        $(impl_basic! { $tt }) +
    };
}

impl_basic! {
    ApiEvent, RuntimeEvent, generated_ids::MessageId,
    generated_ids::ProgramId, generated_ids::CodeId, generated_ids::ReservationId,
    Reason<UserMessageReadRuntimeReason, UserMessageReadSystemReason>
}

impl From<RuntimeCall> for Value {
    fn from(call: RuntimeCall) -> Value {
        match call {
            RuntimeCall::Gear(gear_call) => gear_call_to_scale_value(gear_call),
            RuntimeCall::Sudo(sudo_call) => sudo_call_to_scale_value(sudo_call),
            RuntimeCall::Balances(balances_call) => balances_call_to_scale_value(balances_call),
            RuntimeCall::System(system_call) => system_call_to_scale_value(system_call),
            _ => unimplemented!("other calls aren't supported for now."),
        }
    }
}

fn gear_call_to_scale_value(call: GearCall) -> Value {
    let variant = match call {
        GearCall::upload_code { code } => {
            Value::named_variant("upload_code", [("code", Value::from_bytes(code))])
        }
        GearCall::upload_program {
            code,
            salt,
            init_payload,
            gas_limit,
            value,
        } => Value::named_variant(
            "upload_program",
            [
                ("code", Value::from_bytes(code)),
                ("salt", Value::from_bytes(salt)),
                ("init_payload", Value::from_bytes(init_payload)),
                ("gas_limit", Value::u128(gas_limit as u128)),
                ("value", Value::u128(value as u128)),
            ],
        ),
        GearCall::create_program {
            code_id,
            salt,
            init_payload,
            gas_limit,
            value,
        } => Value::named_variant(
            "create_program",
            [
                ("code_id", Value::from_bytes(code_id.0)),
                ("salt", Value::from_bytes(salt)),
                ("init_payload", Value::from_bytes(init_payload)),
                ("gas_limit", Value::u128(gas_limit as u128)),
                ("value", Value::u128(value as u128)),
            ],
        ),
        GearCall::send_message {
            destination,
            payload,
            gas_limit,
            value,
        } => Value::named_variant(
            "send_message",
            [
                ("destination", Value::from_bytes(destination.0)),
                ("payload", Value::from_bytes(payload)),
                ("gas_limit", Value::u128(gas_limit as u128)),
                ("value", Value::u128(value as u128)),
            ],
        ),
        GearCall::send_reply {
            reply_to_id,
            payload,
            gas_limit,
            value,
        } => Value::named_variant(
            "send_reply",
            [
                ("reply_to_id", Value::from_bytes(reply_to_id.0)),
                ("payload", Value::from_bytes(payload)),
                ("gas_limit", Value::u128(gas_limit as u128)),
                ("value", Value::u128(value as u128)),
            ],
        ),
        GearCall::claim_value { message_id } => Value::named_variant(
            "claim_value",
            [("message_id", Value::from_bytes(message_id.0))],
        ),
        _ => {
            unimplemented!("calls that won't be used in batch call");
        }
    };

    Value::unnamed_variant("Gear", [variant])
}

fn sudo_call_to_scale_value(call: SudoCall) -> Value {
    let variant = match call {
        SudoCall::sudo_unchecked_weight { call, weight } => Value::named_variant(
            "sudo_unchecked_weight",
            [
                ("call", (*call).into()),
                (
                    "weight",
                    Value::named_composite([
                        ("ref_time", Value::u128(weight.ref_time as u128)),
                        ("proof_size", Value::u128(weight.proof_size as u128)),
                    ]),
                ),
            ],
        ),
        _ => unimplemented!("calls that won't be used in batch call"),
    };

    Value::unnamed_variant("Sudo", [variant])
}

fn balances_call_to_scale_value(call: BalancesCall) -> Value {
    let variant = match call {
        BalancesCall::set_balance {
            who,
            new_free,
            new_reserved,
        } => {
            let id = match who {
                MultiAddress::Id(id) => id,
                _ => unreachable!("internal error: unused multi-address variant occurred"),
            };
            Value::named_variant(
                "set_balance",
                [
                    ("who", Value::unnamed_variant("Id", [Value::from_bytes(id)])),
                    ("new_free", Value::u128(new_free)),
                    ("new_reserved", Value::u128(new_reserved)),
                ],
            )
        }
        _ => unreachable!("calls that won't be used in batch call"),
    };

    Value::unnamed_variant("Balances", [variant])
}

fn system_call_to_scale_value(call: SystemCall) -> Value {
    let variant = match call {
        SystemCall::set_storage { items } => {
            let items_as_values: Vec<Value> = items
                .iter()
                .map(|i| {
                    Value::unnamed_composite([Value::from_bytes(&i.0), Value::from_bytes(&i.1)])
                })
                .collect();
            Value::named_variant(
                "set_storage",
                [("items", Value::unnamed_composite(items_as_values))],
            )
        }
        _ => unreachable!("other calls aren't supported for now."),
    };

    Value::unnamed_variant("System", [variant])
}
