// This file is part of Gear.

// Copyright (C) 2021-2024 Gear Technologies Inc.
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

// NOTE: Don't use `gstd` here with `wasm-wrapper` feature enabled.
use crate::{Arg, Call};
use alloc::{string::ToString, vec, vec::Vec};
use core::{fmt::Debug, ops::Deref};
use parity_scale_codec::{WrapperTypeDecode, WrapperTypeEncode};

#[derive(Default, Debug, Clone)]
/// Represent builder across vector of calls to be executed with some entry point.
pub struct Calls(Vec<Call>);

impl From<Call> for Calls {
    fn from(call: Call) -> Self {
        Self(vec![call])
    }
}

impl From<Vec<Call>> for Calls {
    fn from(calls: Vec<Call>) -> Self {
        Self(calls)
    }
}

impl Deref for Calls {
    type Target = Vec<Call>;

    fn deref(&self) -> &Self::Target {
        self.0.as_ref()
    }
}

impl WrapperTypeEncode for Calls {}
impl WrapperTypeDecode for Calls {
    type Wrapped = Vec<Call>;
}

impl Calls {
    pub fn builder() -> Self {
        Default::default()
    }

    pub(crate) fn calls(self) -> Vec<Call> {
        self.0
    }

    pub fn add_call(mut self, call: Call) -> Self {
        self.0.push(call);
        self
    }

    pub fn add_from_iter(mut self, calls: impl Iterator<Item = Call>) -> Self {
        self.0.extend(calls);
        self
    }

    pub fn add_many<const N: usize>(self, calls: [Call; N]) -> Self {
        self.add_from_iter(calls.into_iter())
    }

    pub fn vec(self, key: impl AsRef<str>, value: impl AsRef<[u8]>) -> Self {
        self.add_call(Call::Vec(value.as_ref().to_vec()))
            .store_vec(key)
    }

    pub fn bool(self, key: impl AsRef<str>, value: impl Into<bool>) -> Self {
        self.add_call(Call::Bool(value.into())).store(key)
    }

    pub fn store(self, key: impl AsRef<str>) -> Self {
        self.add_call(Call::Store(key.as_ref().to_string()))
    }

    pub fn store_vec(self, key: impl AsRef<str>) -> Self {
        self.add_call(Call::StoreVec(key.as_ref().to_string()))
    }

    pub fn source(self, key: impl AsRef<str>) -> Self {
        self.add_call(Call::Source).store(key)
    }

    pub fn reply_code(self, key: impl AsRef<str>) -> Self {
        self.add_call(Call::ReplyCode).store_vec(key)
    }

    pub fn value(self, key: impl AsRef<str>) -> Self {
        self.add_call(Call::Value).store(key)
    }

    pub fn value_available(self, key: impl AsRef<str>) -> Self {
        self.add_call(Call::ValueAvailable).store(key)
    }

    pub fn value_as_vec(self, key: impl AsRef<str>) -> Self {
        self.add_call(Call::Value).store_vec(key)
    }

    pub fn value_available_as_vec(self, key: impl AsRef<str>) -> Self {
        self.add_call(Call::ValueAvailable).store_vec(key)
    }

    pub fn message_id(self, key: impl AsRef<str>) -> Self {
        self.add_call(Call::MessageId).store(key)
    }

    pub fn message_id_as_vec(self, key: impl AsRef<str>) -> Self {
        self.add_call(Call::MessageId).store_vec(key)
    }

    pub fn reservation_send_value(
        self,
        reservation: impl Into<Arg<[u8; 32]>>,
        destination: impl Into<Arg<[u8; 32]>>,
        payload: impl Into<Arg<Vec<u8>>>,
        value: impl Into<Arg<u128>>,
    ) -> Self {
        self.add_call(Call::ReservationSend(
            reservation.into(),
            destination.into(),
            payload.into(),
            value.into(),
            0.into(),
        ))
    }

    pub fn send(
        self,
        destination: impl Into<Arg<[u8; 32]>>,
        payload: impl Into<Arg<Vec<u8>>>,
    ) -> Self {
        self.send_value(destination, payload, 0)
    }

    pub fn send_value(
        self,
        destination: impl Into<Arg<[u8; 32]>>,
        payload: impl Into<Arg<Vec<u8>>>,
        value: impl Into<Arg<u128>>,
    ) -> Self {
        self.add_call(Call::Send(
            destination.into(),
            payload.into(),
            None,
            value.into(),
            0.into(),
        ))
    }

    pub fn send_wgas<T: TryInto<u64>>(
        self,
        destination: impl Into<Arg<[u8; 32]>>,
        payload: impl Into<Arg<Vec<u8>>>,
        gas_limit: T,
    ) -> Self
    where
        T::Error: Debug,
    {
        self.send_value_wgas(destination, payload, gas_limit, 0)
    }

    pub fn send_value_wgas<T: TryInto<u64>>(
        self,
        destination: impl Into<Arg<[u8; 32]>>,
        payload: impl Into<Arg<Vec<u8>>>,
        gas_limit: T,
        value: impl Into<Arg<u128>>,
    ) -> Self
    where
        T::Error: Debug,
    {
        let gas_limit = gas_limit
            .try_into()
            .expect("Cannot convert given gas limit into `u64`");
        self.add_call(Call::Send(
            destination.into(),
            payload.into(),
            Some(gas_limit.into()),
            value.into(),
            0.into(),
        ))
    }

    pub fn create_program(
        self,
        code_id: impl Into<Arg<[u8; 32]>>,
        salt: impl Into<Arg<Vec<u8>>>,
        payload: impl Into<Arg<Vec<u8>>>,
    ) -> Self {
        self.create_program_value(code_id, salt, payload, 0)
    }

    pub fn create_program_value(
        self,
        code_id: impl Into<Arg<[u8; 32]>>,
        salt: impl Into<Arg<Vec<u8>>>,
        payload: impl Into<Arg<Vec<u8>>>,
        value: impl Into<Arg<u128>>,
    ) -> Self {
        self.add_call(Call::CreateProgram(
            code_id.into(),
            salt.into(),
            payload.into(),
            None,
            value.into(),
            0.into(),
        ))
    }

    pub fn create_program_wgas<T: TryInto<u64>>(
        self,
        code_id: impl Into<Arg<[u8; 32]>>,
        salt: impl Into<Arg<Vec<u8>>>,
        payload: impl Into<Arg<Vec<u8>>>,
        gas_limit: T,
    ) -> Self
    where
        T::Error: Debug,
    {
        self.create_program_value_wgas(code_id, salt, payload, gas_limit, 0)
    }

    pub fn create_program_value_wgas<T: TryInto<u64>>(
        self,
        code_id: impl Into<Arg<[u8; 32]>>,
        salt: impl Into<Arg<Vec<u8>>>,
        payload: impl Into<Arg<Vec<u8>>>,
        gas_limit: T,
        value: impl Into<Arg<u128>>,
    ) -> Self
    where
        T::Error: Debug,
    {
        let gas_limit = gas_limit
            .try_into()
            .expect("Cannot convert given gas limit into `u64`");
        self.add_call(Call::CreateProgram(
            code_id.into(),
            salt.into(),
            payload.into(),
            Some(gas_limit.into()),
            value.into(),
            0.into(),
        ))
    }

    pub fn reply_deposit<T: TryInto<u64>>(
        self,
        message_id: impl Into<Arg<[u8; 32]>>,
        gas_limit: T,
    ) -> Self
    where
        T::Error: Debug,
    {
        let gas_limit = gas_limit
            .try_into()
            .expect("Cannot convert given gas limit into `u64`");
        self.add_call(Call::ReplyDeposit(message_id.into(), gas_limit.into()))
    }

    pub fn reply(self, payload: impl Into<Arg<Vec<u8>>>) -> Self {
        self.reply_value(payload, 0)
    }

    pub fn reply_value(
        self,
        payload: impl Into<Arg<Vec<u8>>>,
        value: impl Into<Arg<u128>>,
    ) -> Self {
        self.add_call(Call::Reply(payload.into(), None, value.into()))
    }

    pub fn reply_wgas<T: TryInto<u64>>(self, payload: impl Into<Arg<Vec<u8>>>, gas_limit: T) -> Self
    where
        T::Error: Debug,
    {
        self.reply_value_wgas(payload, gas_limit, 0)
    }

    pub fn reply_value_wgas<T: TryInto<u64>>(
        self,
        payload: impl Into<Arg<Vec<u8>>>,
        gas_limit: T,
        value: impl Into<Arg<u128>>,
    ) -> Self
    where
        T::Error: Debug,
    {
        let gas_limit = gas_limit
            .try_into()
            .expect("Cannot convert given gas limit into `u64`");
        self.add_call(Call::Reply(
            payload.into(),
            Some(gas_limit.into()),
            value.into(),
        ))
    }

    pub fn panic(self, message: impl Into<Option<&'static str>>) -> Self {
        self.add_call(Call::Panic(message.into().map(ToString::to_string)))
    }

    pub fn exit(self, inheritor: impl Into<Arg<[u8; 32]>>) -> Self {
        self.add_call(Call::Exit(inheritor.into()))
    }

    pub fn wait(self) -> Self {
        self.add_call(Call::Wait)
    }

    pub fn wait_for(self, duration: impl Into<Arg<u32>>) -> Self {
        self.add_call(Call::WaitFor(duration.into()))
    }

    pub fn wake(self, message_id: impl Into<Arg<[u8; 32]>>) -> Self {
        self.add_call(Call::Wake(message_id.into()))
    }

    pub fn bytes_eq(
        self,
        key: impl AsRef<str>,
        left: impl Into<Arg<Vec<u8>>>,
        right: impl Into<Arg<Vec<u8>>>,
    ) -> Self {
        self.add_call(Call::BytesEq(left.into(), right.into()))
            .store(key)
    }

    pub fn noop(self) -> Self {
        self.add_call(Call::Noop)
    }

    // TODO: support multiple calls for branches by passing mut ref instead of moving value in Call processing.
    #[track_caller]
    pub fn if_else(
        self,
        bool_arg: impl Into<Arg<bool>>,
        true_calls: Self,
        false_calls: Self,
    ) -> Self {
        self.add_call(Call::IfElse(
            bool_arg.into(),
            true_calls.calls(),
            false_calls.calls(),
        ))
    }

    pub fn load(self, key: impl AsRef<str>) -> Self {
        self.add_call(Call::Load).store(key)
    }

    pub fn load_bytes(self, key: impl AsRef<str>) -> Self {
        self.add_call(Call::LoadBytes).store(key)
    }

    pub fn infinite_loop(self) -> Self {
        self.add_call(Call::Loop)
    }

    pub fn system_reserve_gas(self, gas: impl Into<Arg<u64>>) -> Self {
        self.add_call(Call::SystemReserveGas(gas.into()))
    }

    pub fn reserve_gas(self, gas: impl Into<Arg<u64>>, duration: impl Into<Arg<u32>>) -> Self {
        self.add_call(Call::ReserveGas(gas.into(), duration.into()))
    }

    pub fn unreserve_gas(self, reservation_id: impl Into<Arg<[u8; 32]>>) -> Self {
        self.add_call(Call::UnreserveGas(reservation_id.into()))
    }

    pub fn write_in_loop(self, count: impl Into<Arg<u64>>) -> Self {
        self.add_call(Call::WriteN(count.into()))
    }
}
