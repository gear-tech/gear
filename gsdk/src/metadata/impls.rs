// This file is part of Gear.
//
// Copyright (C) 2021-2025 Gear Technologies Inc.
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

use super::{
    runtime_types::{
        frame_system::pallet::Call as SystemCall,
        gear_common::{
            event::*,
            gas_provider::node::{GasNodeId, NodeLock},
        },
        gear_core::message as generated_message,
        gear_core_errors as generated_core_errors, gprimitives as generated_ids,
        pallet_balances::pallet::Call as BalancesCall,
        pallet_gear::pallet::Call as GearCall,
        pallet_gear_voucher::internal::{PrepaidCall, VoucherId},
        pallet_sudo::pallet::Call as SudoCall,
    },
    vara_runtime::{RuntimeCall, RuntimeEvent},
};
use core::ops::{Index, IndexMut};
use gear_core::{ids, message, message::UserMessage};
use parity_scale_codec::{Decode, Encode};
use subxt::{dynamic::Value, utils::MultiAddress};

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

impl From<ids::ActorId> for generated_ids::ActorId {
    fn from(other: ids::ActorId) -> Self {
        Self(other.into())
    }
}

impl From<generated_ids::ActorId> for ids::ActorId {
    fn from(other: generated_ids::ActorId) -> Self {
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

impl From<generated_core_errors::simple::ReplyCode> for gear_core_errors::ReplyCode {
    fn from(value: generated_core_errors::simple::ReplyCode) -> Self {
        Self::decode(&mut value.encode().as_ref()).expect("Incompatible metadata")
    }
}

impl From<generated_message::common::ReplyDetails> for message::ReplyDetails {
    fn from(other: generated_message::common::ReplyDetails) -> Self {
        message::ReplyDetails::new(other.to.into(), other.code.into())
    }
}

impl From<generated_message::user::UserMessage> for message::UserMessage {
    fn from(other: generated_message::user::UserMessage) -> Self {
        message::UserMessage::new(
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

impl From<generated_message::user::UserStoredMessage> for message::UserStoredMessage {
    fn from(other: generated_message::user::UserStoredMessage) -> Self {
        message::UserStoredMessage::new(
            other.id.into(),
            other.source.into(),
            other.destination.into(),
            // converting data from the same type
            other.payload.0.try_into().expect("Infallible"),
            other.value,
        )
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
    ApiEvent, generated_ids::MessageId,
    generated_ids::ActorId, generated_ids::CodeId, generated_ids::ReservationId,
    Reason<UserMessageReadRuntimeReason, UserMessageReadSystemReason>,
    generated_core_errors::simple::ReplyCode, VoucherId
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
            keep_alive,
        } => Value::named_variant(
            "upload_program",
            [
                ("code", Value::from_bytes(code)),
                ("salt", Value::from_bytes(salt)),
                ("init_payload", Value::from_bytes(init_payload)),
                ("gas_limit", Value::u128(gas_limit as u128)),
                ("value", Value::u128(value as u128)),
                ("keep_alive", Value::bool(keep_alive)),
            ],
        ),
        GearCall::create_program {
            code_id,
            salt,
            init_payload,
            gas_limit,
            value,
            keep_alive,
        } => Value::named_variant(
            "create_program",
            [
                ("code_id", Value::from_bytes(code_id.0)),
                ("salt", Value::from_bytes(salt)),
                ("init_payload", Value::from_bytes(init_payload)),
                ("gas_limit", Value::u128(gas_limit as u128)),
                ("value", Value::u128(value as u128)),
                ("keep_alive", Value::bool(keep_alive)),
            ],
        ),
        GearCall::send_message {
            destination,
            payload,
            gas_limit,
            value,
            keep_alive,
        } => Value::named_variant(
            "send_message",
            [
                ("destination", Value::from_bytes(destination.0)),
                ("payload", Value::from_bytes(payload)),
                ("gas_limit", Value::u128(gas_limit as u128)),
                ("value", Value::u128(value as u128)),
                ("keep_alive", Value::bool(keep_alive)),
            ],
        ),
        GearCall::send_reply {
            reply_to_id,
            payload,
            gas_limit,
            value,
            keep_alive,
        } => Value::named_variant(
            "send_reply",
            [
                ("reply_to_id", Value::from_bytes(reply_to_id.0)),
                ("payload", Value::from_bytes(payload)),
                ("gas_limit", Value::u128(gas_limit as u128)),
                ("value", Value::u128(value as u128)),
                ("keep_alive", Value::bool(keep_alive)),
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
        BalancesCall::force_set_balance { who, new_free } => {
            let id = match who {
                MultiAddress::Id(id) => id,
                _ => unreachable!("internal error: unused multi-address variant occurred"),
            };
            Value::named_variant(
                "force_set_balance",
                [
                    ("who", Value::unnamed_variant("Id", [Value::from_bytes(id)])),
                    ("new_free", Value::u128(new_free)),
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
        SystemCall::set_code { code } => {
            Value::named_variant("set_code", [("code", Value::from_bytes(code))])
        }
        SystemCall::set_code_without_checks { code } => Value::named_variant(
            "set_code_without_checks",
            [("code", Value::from_bytes(code))],
        ),
        _ => unreachable!("other calls aren't supported for now."),
    };

    Value::unnamed_variant("System", [variant])
}

/// Convert to type.
pub trait Convert<T> {
    fn convert(self) -> T;
}

impl Convert<subxt::utils::AccountId32> for sp_runtime::AccountId32 {
    fn convert(self) -> subxt::utils::AccountId32 {
        let hash: &[u8; 32] = self.as_ref();
        subxt::utils::AccountId32::from(*hash)
    }
}

impl Convert<subxt::utils::MultiAddress<subxt::utils::AccountId32, ()>>
    for sp_runtime::MultiAddress<sp_runtime::AccountId32, ()>
{
    fn convert(self) -> subxt::utils::MultiAddress<subxt::utils::AccountId32, ()> {
        match self {
            sp_runtime::MultiAddress::Address20(id) => subxt::utils::MultiAddress::Address20(id),
            sp_runtime::MultiAddress::Address32(id) => subxt::utils::MultiAddress::Address32(id),
            sp_runtime::MultiAddress::Id(id) => subxt::utils::MultiAddress::Id(id.convert()),
            sp_runtime::MultiAddress::Index(index) => subxt::utils::MultiAddress::Index(index),
            sp_runtime::MultiAddress::Raw(raw) => subxt::utils::MultiAddress::Raw(raw),
        }
    }
}

impl From<PrepaidCall<u128>> for Value {
    fn from(call: PrepaidCall<u128>) -> Value {
        prepaid_call_to_scale_value(call)
    }
}

fn prepaid_call_to_scale_value(call: PrepaidCall<u128>) -> Value {
    match call {
        PrepaidCall::SendMessage {
            destination,
            payload,
            gas_limit,
            value,
            keep_alive,
        } => Value::named_variant(
            "SendMessage",
            [
                ("destination", Value::from_bytes(destination.0)),
                ("payload", Value::from_bytes(payload)),
                ("gas_limit", Value::u128(gas_limit as u128)),
                ("value", Value::u128(value as u128)),
                ("keep_alive", Value::bool(keep_alive)),
            ],
        ),
        PrepaidCall::SendReply {
            reply_to_id,
            payload,
            gas_limit,
            value,
            keep_alive,
        } => Value::named_variant(
            "SendReply",
            [
                ("reply_to_id", Value::from_bytes(reply_to_id.0)),
                ("payload", Value::from_bytes(payload)),
                ("gas_limit", Value::u128(gas_limit as u128)),
                ("value", Value::u128(value as u128)),
                ("keep_alive", Value::bool(keep_alive)),
            ],
        ),
        PrepaidCall::UploadCode { code } => {
            Value::named_variant("UploadCode", [("code", Value::from_bytes(code))])
        }
        PrepaidCall::DeclineVoucher => Value::unnamed_variant("DeclineVoucher", []),
        _ => unreachable!("other prepaid calls aren't supported"),
    }
}

impl Convert<Value<()>> for Option<Value<()>> {
    fn convert(self) -> Value<()> {
        match self {
            Some(v) => Value::unnamed_variant("Some", [v]),
            None => Value::unnamed_variant("None", []),
        }
    }
}
