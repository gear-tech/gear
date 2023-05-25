// This file is part of Gear.

// Copyright (C) 2021-2023 Gear Technologies Inc.
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

//! This module provides structs for building calls in smart contracts
//! through a fluent methods.

use crate::{
    errors::{ContractError, Result},
    marker::PhantomData,
    msg::{self, CodecCreateProgramFuture, CodecMessageFuture, CreateProgramFuture, MessageFuture},
    prelude::{
        convert::AsRef,
        ops::{RangeBounds, RangeFull},
    },
    prog::ProgramGenerator,
    ActorId, CodeId, MessageId, ReservationId,
};
use scale_info::scale::{Decode, Encode};

/// Describes the type of call from the smart contract.
enum CallType {
    CreateProgram(CodeId),
    SendMessage(ActorId),
    ReplyMessage,
}

/// Describes all possible payload types.
enum MessagePayload<'a, Buffer, Encodable, Range>
where
    Buffer: AsRef<[u8]>,
    Encodable: Encode,
    Range: RangeBounds<usize> + Copy,
{
    /// used by functions like `msg::send_bytes()`.
    Bytes(&'a Buffer),
    /// used by functions like `msg::send<E: Encode>()`.
    Encode(&'a Encodable),
    /// used by functions like `msg::send_input<R: RangeBounds<usize>>()`.
    Input(Range),
}

/// Contains the call type and possible arguments for making this call.
struct FinalizedMessage<'a, Buffer, Encodable, Range>
where
    Buffer: AsRef<[u8]>,
    Encodable: Encode,
    Range: RangeBounds<usize> + Copy,
{
    call_type: CallType,
    payload: MessagePayload<'a, Buffer, Encodable, Range>,
    value: u128,
    delay: Option<u32>,
    gas_limit: Option<u64>,
    reservation_id: Option<ReservationId>,
}

/// This data type provides a more convenient wrapper over functions
/// such as `msg::send_bytes()`, `msg::reply_bytes()`.
pub type BytesMessageBuilder<'a, Buffer> = MessageBuilder<'a, Buffer, (), RangeFull>;

/// Create a new [`BytesMessageBuilder`].
pub fn bytes<'a, Buffer: AsRef<[u8]>>() -> BytesMessageBuilder<'a, Buffer> {
    BytesMessageBuilder::new()
}

/// This data type provides a more convenient wrapper over functions
/// such as `msg::send<E: Encode>()`, `msg::reply<E: Encode>()`.
pub type EncodeMessageBuilder<'a, Encodable> = MessageBuilder<'a, [u8; 0], Encodable, RangeFull>;

/// Create a new [`EncodeMessageBuilder`].
pub fn encode<'a, Encodable: Encode>() -> EncodeMessageBuilder<'a, Encodable> {
    EncodeMessageBuilder::new()
}

/// This data type provides a more convenient wrapper over functions
/// such as `msg::send_input<R: RangeBounds<usize>>()`, `msg::reply_input<R:
/// ...>()`.
pub type InputMessageBuilder<'a, Range> = MessageBuilder<'a, [u8; 0], (), Range>;

/// Create a new [`InputMessageBuilder`].
pub fn input<'a, Range: RangeBounds<usize> + Copy>() -> InputMessageBuilder<'a, Range> {
    InputMessageBuilder::new()
}

/// Provides an alternative way to interact with the messages API.
///
/// Unlike the traditional way of using imperative functions,
/// it allows calls to be made through a fluent methods.
///
/// Instead of writing imperative code like this:
/// ```no_run
/// use gstd::{msg, ActorId};
///
/// msg::send_bytes_with_gas_delayed(ActorId::zero(), b"PING", 1_000_000, 0, 60)
///     .expect("failed to send msg");
/// ```
///
/// This wrapper allows you to write code like this:
/// ```no_run
/// use gstd::{builder, ActorId};
///
/// builder::bytes::<_>()
///     .to(ActorId::zero())
///     .payload_bytes(b"PING")
///     .value(0)
///     .with_gas(1_000_000)
///     .delayed(60)
///     .send()
///     .expect("failed to send msg");
/// ```
pub struct MessageBuilder<'a, Buffer, Encodable, Range>
where
    Buffer: AsRef<[u8]>,
    Encodable: Encode,
    Range: RangeBounds<usize> + Copy,
{
    code_id: Option<CodeId>,
    program: Option<ActorId>,
    payload: Option<MessagePayload<'a, Buffer, Encodable, Range>>,
    value: Option<u128>,
    delay: Option<u32>,
    gas_limit: Option<u64>,
    reservation_id: Option<ReservationId>,
}

impl<'a, Buffer, Encodable, Range> MessageBuilder<'a, Buffer, Encodable, Range>
where
    Buffer: AsRef<[u8]>,
    Encodable: Encode,
    Range: RangeBounds<usize> + Copy,
{
    /// Create a new [`MessageBuilder`] with zeroed fields.
    pub const fn new() -> Self {
        Self {
            code_id: None,
            program: None,
            payload: None,
            value: None,
            delay: None,
            gas_limit: None,
            reservation_id: None,
        }
    }

    /// Sets `code_id` which can be used to call
    /// [`create_program`](Self::create_program) later.
    pub const fn code_id(mut self, code_id: CodeId) -> Self {
        self.code_id = Some(code_id);
        self
    }

    /// Sets `program` that should receive the message.
    pub const fn to(mut self, program: ActorId) -> Self {
        self.program = Some(program);
        self
    }

    /// Sets `payload` that can be interpreted as a byte buffer.
    pub const fn payload_bytes(mut self, payload: &'a Buffer) -> Self {
        self.payload = Some(MessagePayload::Bytes(payload));
        self
    }

    /// Sets `payload` that implements [`Encode`] trait.
    pub const fn payload_encode(mut self, payload: &'a Encodable) -> Self {
        self.payload = Some(MessagePayload::Encode(payload));
        self
    }

    /// Sets `payload` that can be used for [`msg::send_input`] functions.
    pub const fn payload_input(mut self, payload: Range) -> Self {
        self.payload = Some(MessagePayload::Input(payload));
        self
    }

    /// Sets `value` which is the amount of native tokens.
    pub const fn value(mut self, value: u128) -> Self {
        self.value = Some(value);
        self
    }

    /// Sets `delay`, expressed in number of blocks.
    pub const fn delayed(mut self, delay: u32) -> Self {
        self.delay = Some(delay);
        self
    }

    /// Sets explicit `gas_limit`.
    pub const fn with_gas(mut self, gas_limit: u64) -> Self {
        self.gas_limit = Some(gas_limit);
        self
    }

    /// Sets `reservation_id` where gas will be spent from.
    pub const fn from_reservation(mut self, reservation_id: ReservationId) -> Self {
        self.reservation_id = Some(reservation_id);
        self
    }

    /// This internal function performs some basic integrity checks.
    /// Also this is a constant function that allows the rust compiler to
    /// perform optimizations.
    const fn finalize(self) -> Result<FinalizedMessage<'a, Buffer, Encodable, Range>> {
        let call_type = match (self.code_id, self.program) {
            (None, None) => CallType::ReplyMessage,
            (None, Some(program)) => CallType::SendMessage(program),
            (Some(code_id), None) => CallType::CreateProgram(code_id),
            (Some(_), Some(_)) => {
                return Err(ContractError::BuilderUsage("failed to determine call_type"))
            }
        };

        let Some(payload) = self.payload else {
            return Err(ContractError::BuilderUsage(
                "you must initialize payload using one of the `.payload_*(_)`. MessageBuilder methods"
            ));
        };

        // there is no `gstd::msg::send_input` function with `reservation_id`
        if matches!(payload, MessagePayload::Input(_)) && self.reservation_id.is_some() {
            return Err(ContractError::BuilderUsage(
                "you can't use `.payload_input(_)` and `.from_reservation(_)` together",
            ));
        }

        // there is no `gstd::prog::create_program` function with `reservation_id`
        if matches!(call_type, CallType::CreateProgram(_)) && self.reservation_id.is_some() {
            return Err(ContractError::BuilderUsage(
                "you can't use `.create_program()` and `.from_reservation(_)` together",
            ));
        }

        // there is no way to create program from other types of payload
        if matches!(call_type, CallType::CreateProgram(_))
            && !matches!(payload, MessagePayload::Bytes(_))
        {
            return Err(ContractError::BuilderUsage(
                "you can use `.payload_bytes(_)` only in this case",
            ));
        }

        // fallback to 0 if not set
        let value = match self.value {
            Some(value) => value,
            None => 0,
        };

        let (delay, gas_limit, reservation_id) = (self.delay, self.gas_limit, self.reservation_id);

        Ok(FinalizedMessage {
            call_type,
            payload,
            value,
            delay,
            gas_limit,
            reservation_id,
        })
    }

    /// Returns a wrapper that works with `*_for_reply` functions.
    #[inline(always)]
    pub fn for_reply(self) -> MessageBuilderForReply<'a, Buffer, Encodable, Range> {
        MessageBuilderForReply { inner: self }
    }

    /// Returns a wrapper that works with `*_for_reply_as::<D: Decode>`
    /// functions.
    #[inline(always)]
    pub fn for_reply_as<Decodable: Decode>(
        self,
    ) -> MessageBuilderForReplyAs<'a, Buffer, Encodable, Range, Decodable> {
        MessageBuilderForReplyAs {
            inner: self,
            phantom: PhantomData,
        }
    }

    /// Tries to call one of the `msg::send*` functions depending
    /// on how the arguments were constructed.
    #[inline(always)]
    pub fn send(self) -> Result<MessageId> {
        let FinalizedMessage {
            call_type,
            payload,
            value,
            delay,
            gas_limit,
            reservation_id,
        } = self.finalize()?;

        let CallType::SendMessage(program) = call_type else {
            return Err(ContractError::BuilderUsage("unexpected call_type"));
        };

        match (delay, gas_limit, reservation_id) {
            (None, None, None) => match payload {
                MessagePayload::Bytes(payload) => msg::send_bytes(program, payload, value),
                MessagePayload::Encode(payload) => {
                    msg::send_bytes(program, payload.encode(), value)
                }
                MessagePayload::Input(range) => msg::send_input(program, value, range),
            },
            (None, None, Some(reservation_id)) => match payload {
                MessagePayload::Bytes(payload) => {
                    msg::send_bytes_from_reservation(reservation_id, program, payload, value)
                }
                MessagePayload::Encode(payload) => msg::send_bytes_from_reservation(
                    reservation_id,
                    program,
                    payload.encode(),
                    value,
                ),
                MessagePayload::Input(_) => unreachable!(),
            },
            (None, Some(gas_limit), None) => match payload {
                MessagePayload::Bytes(payload) => {
                    msg::send_bytes_with_gas(program, payload, gas_limit, value)
                }
                MessagePayload::Encode(payload) => {
                    msg::send_bytes_with_gas(program, payload.encode(), gas_limit, value)
                }
                MessagePayload::Input(range) => {
                    msg::send_input_with_gas(program, gas_limit, value, range)
                }
            },
            (None, Some(_), Some(_)) => unreachable!(),
            (Some(delay), None, None) => match payload {
                MessagePayload::Bytes(payload) => {
                    msg::send_bytes_delayed(program, payload, value, delay)
                }
                MessagePayload::Encode(payload) => {
                    msg::send_bytes_delayed(program, payload.encode(), value, delay)
                }
                MessagePayload::Input(range) => {
                    msg::send_input_delayed(program, value, range, delay)
                }
            },
            (Some(delay), None, Some(reservation_id)) => match payload {
                MessagePayload::Bytes(payload) => msg::send_bytes_delayed_from_reservation(
                    reservation_id,
                    program,
                    payload,
                    value,
                    delay,
                ),
                MessagePayload::Encode(payload) => msg::send_bytes_delayed_from_reservation(
                    reservation_id,
                    program,
                    payload.encode(),
                    value,
                    delay,
                ),
                MessagePayload::Input(_) => unreachable!(),
            },
            (Some(delay), Some(gas_limit), None) => match payload {
                MessagePayload::Bytes(payload) => {
                    msg::send_bytes_with_gas_delayed(program, payload, gas_limit, value, delay)
                }
                MessagePayload::Encode(payload) => msg::send_bytes_with_gas_delayed(
                    program,
                    payload.encode(),
                    gas_limit,
                    value,
                    delay,
                ),
                MessagePayload::Input(range) => {
                    msg::send_input_with_gas_delayed(program, gas_limit, value, range, delay)
                }
            },
            (Some(_), Some(_), Some(_)) => unreachable!(),
        }
    }

    /// Tries to call one of the `msg::reply*` functions depending
    /// on how the arguments were constructed.
    #[inline(always)]
    pub fn reply(self) -> Result<MessageId> {
        let FinalizedMessage {
            call_type,
            payload,
            value,
            delay,
            gas_limit,
            reservation_id,
        } = self.finalize()?;

        let CallType::ReplyMessage = call_type else {
            return Err(ContractError::BuilderUsage("unexpected call_type"));
        };

        match (delay, gas_limit, reservation_id) {
            (None, None, None) => match payload {
                MessagePayload::Bytes(payload) => msg::reply_bytes(payload, value),
                MessagePayload::Encode(payload) => msg::reply_bytes(payload.encode(), value),
                MessagePayload::Input(range) => msg::reply_input(value, range),
            },
            (None, None, Some(reservation_id)) => match payload {
                MessagePayload::Bytes(payload) => {
                    msg::reply_bytes_from_reservation(reservation_id, payload, value)
                }
                MessagePayload::Encode(payload) => {
                    msg::reply_bytes_from_reservation(reservation_id, payload.encode(), value)
                }
                MessagePayload::Input(_) => unreachable!(),
            },
            (None, Some(gas_limit), None) => match payload {
                MessagePayload::Bytes(payload) => {
                    msg::reply_bytes_with_gas(payload, gas_limit, value)
                }
                MessagePayload::Encode(payload) => {
                    msg::reply_bytes_with_gas(payload.encode(), gas_limit, value)
                }
                MessagePayload::Input(range) => msg::reply_input_with_gas(gas_limit, value, range),
            },
            (None, Some(_), Some(_)) => unreachable!(),
            (Some(delay), None, None) => match payload {
                MessagePayload::Bytes(payload) => msg::reply_bytes_delayed(payload, value, delay),
                MessagePayload::Encode(payload) => {
                    msg::reply_bytes_delayed(payload.encode(), value, delay)
                }
                MessagePayload::Input(range) => msg::reply_input_delayed(value, range, delay),
            },
            (Some(delay), None, Some(reservation_id)) => match payload {
                MessagePayload::Bytes(payload) => {
                    msg::reply_bytes_delayed_from_reservation(reservation_id, payload, value, delay)
                }
                MessagePayload::Encode(payload) => msg::reply_bytes_delayed_from_reservation(
                    reservation_id,
                    payload.encode(),
                    value,
                    delay,
                ),
                MessagePayload::Input(_) => unreachable!(),
            },
            (Some(delay), Some(gas_limit), None) => match payload {
                MessagePayload::Bytes(payload) => {
                    msg::reply_bytes_with_gas_delayed(payload, gas_limit, value, delay)
                }
                MessagePayload::Encode(payload) => {
                    msg::reply_bytes_with_gas_delayed(payload.encode(), gas_limit, value, delay)
                }
                MessagePayload::Input(range) => {
                    msg::reply_input_with_gas_delayed(gas_limit, value, range, delay)
                }
            },
            (Some(_), Some(_), Some(_)) => unreachable!(),
        }
    }

    /// Tries to call one of the `ProgramGenerator::create_program*` functions
    /// depending on how the arguments were constructed.
    #[inline(always)]
    pub fn create_program(self) -> Result<(MessageId, ActorId)> {
        let FinalizedMessage {
            call_type,
            payload,
            value,
            delay,
            gas_limit,
            reservation_id,
        } = self.finalize()?;

        let CallType::CreateProgram(code_id) = call_type else {
            return Err(ContractError::BuilderUsage("unexpected call_type"));
        };

        let MessagePayload::Bytes(payload) = payload else {
            return Err(ContractError::BuilderUsage("unexpected payload type"));
        };

        match (delay, gas_limit, reservation_id) {
            (None, None, None) => ProgramGenerator::create_program(code_id, payload, value),
            (None, None, Some(_)) => unreachable!(),
            (None, Some(gas_limit), None) => {
                ProgramGenerator::create_program_with_gas(code_id, payload, gas_limit, value)
            }
            (None, Some(_), Some(_)) => unreachable!(),
            (Some(delay), None, None) => {
                ProgramGenerator::create_program_delayed(code_id, payload, value, delay)
            }
            (Some(_), None, Some(_)) => unreachable!(),
            (Some(delay), Some(gas_limit), None) => {
                ProgramGenerator::create_program_with_gas_delayed(
                    code_id, payload, gas_limit, value, delay,
                )
            }
            (Some(_), Some(_), Some(_)) => unreachable!(),
        }
    }
}

/// Wrapper that works with `*_for_reply` functions.
pub struct MessageBuilderForReply<'a, Buffer, Encodable, Range>
where
    Buffer: AsRef<[u8]>,
    Encodable: Encode,
    Range: RangeBounds<usize> + Copy,
{
    inner: MessageBuilder<'a, Buffer, Encodable, Range>,
}

impl<'a, Buffer, Encodable, Range> MessageBuilderForReply<'a, Buffer, Encodable, Range>
where
    Buffer: AsRef<[u8]>,
    Encodable: Encode,
    Range: RangeBounds<usize> + Copy,
{
    /// Tries to call one of the `msg::send*_for_reply` functions depending
    /// on how the arguments were constructed.
    #[inline(always)]
    pub fn send(self) -> Result<MessageFuture> {
        let FinalizedMessage {
            call_type,
            payload,
            value,
            delay,
            gas_limit,
            reservation_id,
        } = self.inner.finalize()?;

        let CallType::SendMessage(program) = call_type else {
            return Err(ContractError::BuilderUsage("unexpected call_type"));
        };

        match (delay, gas_limit, reservation_id) {
            (None, None, None) => match payload {
                MessagePayload::Bytes(payload) => {
                    msg::send_bytes_for_reply(program, payload, value)
                }
                MessagePayload::Encode(payload) => {
                    msg::send_bytes_for_reply(program, payload.encode(), value)
                }
                MessagePayload::Input(range) => msg::send_input_for_reply(program, value, range),
            },
            (None, None, Some(reservation_id)) => match payload {
                MessagePayload::Bytes(payload) => msg::send_bytes_from_reservation_for_reply(
                    reservation_id,
                    program,
                    payload,
                    value,
                ),
                MessagePayload::Encode(payload) => msg::send_bytes_from_reservation_for_reply(
                    reservation_id,
                    program,
                    payload.encode(),
                    value,
                ),
                MessagePayload::Input(_) => unreachable!(),
            },
            (None, Some(gas_limit), None) => match payload {
                MessagePayload::Bytes(payload) => {
                    msg::send_bytes_with_gas_for_reply(program, payload, gas_limit, value)
                }
                MessagePayload::Encode(payload) => {
                    msg::send_bytes_with_gas_for_reply(program, payload.encode(), gas_limit, value)
                }
                MessagePayload::Input(range) => {
                    msg::send_input_with_gas_for_reply(program, gas_limit, value, range)
                }
            },
            (None, Some(_), Some(_)) => unreachable!(),
            (Some(_), None, None) => unreachable!(),
            (Some(_), None, Some(_)) => unreachable!(),
            (Some(_), Some(_), None) => unreachable!(),
            (Some(_), Some(_), Some(_)) => unreachable!(),
        }
    }

    /// Tries to call one of the `msg::reply*_for_reply` functions depending
    /// on how the arguments were constructed.
    #[inline(always)]
    pub fn reply(self) -> Result<MessageFuture> {
        let FinalizedMessage {
            call_type,
            payload,
            value,
            delay,
            gas_limit,
            reservation_id,
        } = self.inner.finalize()?;

        let CallType::ReplyMessage = call_type else {
            return Err(ContractError::BuilderUsage("unexpected call_type"));
        };

        match (delay, gas_limit, reservation_id) {
            (None, None, None) => match payload {
                MessagePayload::Bytes(payload) => msg::reply_bytes_for_reply(payload, value),
                MessagePayload::Encode(payload) => {
                    msg::reply_bytes_for_reply(payload.encode(), value)
                }
                MessagePayload::Input(range) => msg::reply_input_for_reply(value, range),
            },
            (None, None, Some(reservation_id)) => match payload {
                MessagePayload::Bytes(payload) => {
                    msg::reply_bytes_from_reservation_for_reply(reservation_id, payload, value)
                }
                MessagePayload::Encode(payload) => msg::reply_bytes_from_reservation_for_reply(
                    reservation_id,
                    payload.encode(),
                    value,
                ),
                MessagePayload::Input(_) => unreachable!(),
            },
            (None, Some(gas_limit), None) => match payload {
                MessagePayload::Bytes(payload) => {
                    msg::reply_bytes_with_gas_for_reply(payload, gas_limit, value)
                }
                MessagePayload::Encode(payload) => {
                    msg::reply_bytes_with_gas_for_reply(payload.encode(), gas_limit, value)
                }
                MessagePayload::Input(range) => {
                    msg::reply_input_with_gas_for_reply(gas_limit, value, range)
                }
            },
            (None, Some(_), Some(_)) => unreachable!(),
            (Some(_), None, None) => unreachable!(),
            (Some(_), None, Some(_)) => unreachable!(),
            (Some(_), Some(_), None) => unreachable!(),
            (Some(_), Some(_), Some(_)) => unreachable!(),
        }
    }

    /// Tries to call one of the `ProgramGenerator::create_program*_for_reply`
    /// functions depending on how the arguments were constructed.
    #[inline(always)]
    pub fn create_program(self) -> Result<CreateProgramFuture> {
        let FinalizedMessage {
            call_type,
            payload,
            value,
            delay,
            gas_limit,
            reservation_id,
        } = self.inner.finalize()?;

        let CallType::CreateProgram(code_id) = call_type else {
            return Err(ContractError::BuilderUsage("unexpected call_type"));
        };

        let MessagePayload::Bytes(payload) = payload else {
            return Err(ContractError::BuilderUsage("unexpected payload type"));
        };

        match (delay, gas_limit, reservation_id) {
            (None, None, None) => {
                ProgramGenerator::create_program_for_reply(code_id, payload, value)
            }
            (None, None, Some(_)) => unreachable!(),
            (None, Some(gas_limit), None) => ProgramGenerator::create_program_with_gas_for_reply(
                code_id, payload, gas_limit, value,
            ),
            (None, Some(_), Some(_)) => unreachable!(),
            (Some(_), None, None) => unreachable!(),
            (Some(_), None, Some(_)) => unreachable!(),
            (Some(_), Some(_), None) => unreachable!(),
            (Some(_), Some(_), Some(_)) => unreachable!(),
        }
    }
}

/// Wrapper that works with `*_for_reply_as::<D: Decode>` functions.
pub struct MessageBuilderForReplyAs<'a, Buffer, Encodable, Range, Decodable>
where
    Buffer: AsRef<[u8]>,
    Encodable: Encode,
    Range: RangeBounds<usize> + Copy,
    Decodable: Decode,
{
    inner: MessageBuilder<'a, Buffer, Encodable, Range>,
    phantom: PhantomData<Decodable>,
}

impl<'a, Buffer, Encodable, Range, Decodable>
    MessageBuilderForReplyAs<'a, Buffer, Encodable, Range, Decodable>
where
    Buffer: AsRef<[u8]>,
    Encodable: Encode,
    Range: RangeBounds<usize> + Copy,
    Decodable: Decode,
{
    /// Tries to call one of the `msg::send*_for_reply_as::<D: Decode>`
    /// functions depending on how the arguments were constructed.
    #[inline(always)]
    pub fn send(self) -> Result<CodecMessageFuture<Decodable>> {
        let FinalizedMessage {
            call_type,
            payload,
            value,
            delay,
            gas_limit,
            reservation_id,
        } = self.inner.finalize()?;

        let CallType::SendMessage(program) = call_type else {
            return Err(ContractError::BuilderUsage("unexpected call_type"));
        };

        match (delay, gas_limit, reservation_id) {
            (None, None, None) => match payload {
                MessagePayload::Bytes(payload) => {
                    msg::send_bytes_for_reply_as::<_, Decodable>(program, payload, value)
                }
                MessagePayload::Encode(payload) => {
                    msg::send_bytes_for_reply_as::<_, Decodable>(program, payload.encode(), value)
                }
                MessagePayload::Input(range) => {
                    msg::send_input_for_reply_as::<_, Decodable>(program, value, range)
                }
            },
            (None, None, Some(reservation_id)) => match payload {
                MessagePayload::Bytes(payload) => msg::send_bytes_from_reservation_for_reply_as::<
                    _,
                    Decodable,
                >(
                    reservation_id, program, payload, value
                ),
                MessagePayload::Encode(payload) => {
                    msg::send_bytes_from_reservation_for_reply_as::<_, Decodable>(
                        reservation_id,
                        program,
                        payload.encode(),
                        value,
                    )
                }
                MessagePayload::Input(_) => unreachable!(),
            },
            (None, Some(gas_limit), None) => match payload {
                MessagePayload::Bytes(payload) => {
                    msg::send_bytes_with_gas_for_reply_as::<_, Decodable>(
                        program, payload, gas_limit, value,
                    )
                }
                MessagePayload::Encode(payload) => msg::send_bytes_with_gas_for_reply_as::<
                    _,
                    Decodable,
                >(
                    program, payload.encode(), gas_limit, value
                ),
                MessagePayload::Input(range) => {
                    msg::send_input_with_gas_for_reply_as::<_, Decodable>(
                        program, gas_limit, value, range,
                    )
                }
            },
            (None, Some(_), Some(_)) => unreachable!(),
            (Some(_), None, None) => unreachable!(),
            (Some(_), None, Some(_)) => unreachable!(),
            (Some(_), Some(_), None) => unreachable!(),
            (Some(_), Some(_), Some(_)) => unreachable!(),
        }
    }

    /// Tries to call one of the `msg::reply*_for_reply_as::<D: Decode>`
    /// functions depending on how the arguments were constructed.
    #[inline(always)]
    pub fn reply(self) -> Result<CodecMessageFuture<Decodable>> {
        let FinalizedMessage {
            call_type,
            payload,
            value,
            delay,
            gas_limit,
            reservation_id,
        } = self.inner.finalize()?;

        let CallType::ReplyMessage = call_type else {
            return Err(ContractError::BuilderUsage("unexpected call_type"));
        };

        match (delay, gas_limit, reservation_id) {
            (None, None, None) => match payload {
                MessagePayload::Bytes(payload) => {
                    msg::reply_bytes_for_reply_as::<Decodable>(payload, value)
                }
                MessagePayload::Encode(payload) => {
                    msg::reply_bytes_for_reply_as::<Decodable>(payload.encode(), value)
                }
                MessagePayload::Input(range) => {
                    msg::reply_input_for_reply_as::<_, Decodable>(value, range)
                }
            },
            (None, None, Some(reservation_id)) => match payload {
                MessagePayload::Bytes(payload) => msg::reply_bytes_from_reservation_for_reply_as::<
                    Decodable,
                >(reservation_id, payload, value),
                MessagePayload::Encode(payload) => {
                    msg::reply_bytes_from_reservation_for_reply_as::<Decodable>(
                        reservation_id,
                        payload.encode(),
                        value,
                    )
                }
                MessagePayload::Input(_) => unreachable!(),
            },
            (None, Some(gas_limit), None) => match payload {
                MessagePayload::Bytes(payload) => {
                    msg::reply_bytes_with_gas_for_reply_as::<Decodable>(payload, gas_limit, value)
                }
                MessagePayload::Encode(payload) => msg::reply_bytes_with_gas_for_reply_as::<
                    Decodable,
                >(
                    payload.encode(), gas_limit, value
                ),
                MessagePayload::Input(range) => {
                    msg::reply_input_with_gas_for_reply_as::<_, Decodable>(gas_limit, value, range)
                }
            },
            (None, Some(_), Some(_)) => unreachable!(),
            (Some(_), None, None) => unreachable!(),
            (Some(_), None, Some(_)) => unreachable!(),
            (Some(_), Some(_), None) => unreachable!(),
            (Some(_), Some(_), Some(_)) => unreachable!(),
        }
    }

    /// Tries to call one of the
    /// `ProgramGenerator::create_program*_for_reply_as::<D: Decode>`
    /// functions depending on how the arguments were constructed.
    #[inline(always)]
    pub fn create_program(self) -> Result<CodecCreateProgramFuture<Decodable>> {
        let FinalizedMessage {
            call_type,
            payload,
            value,
            delay,
            gas_limit,
            reservation_id,
        } = self.inner.finalize()?;

        let CallType::CreateProgram(code_id) = call_type else {
            return Err(ContractError::BuilderUsage("unexpected call_type"));
        };

        let MessagePayload::Bytes(payload) = payload else {
            return Err(ContractError::BuilderUsage("unexpected payload type"));
        };

        match (delay, gas_limit, reservation_id) {
            (None, None, None) => {
                ProgramGenerator::create_program_for_reply_as::<Decodable>(code_id, payload, value)
            }
            (None, None, Some(_)) => unreachable!(),
            (None, Some(gas_limit), None) => {
                ProgramGenerator::create_program_with_gas_for_reply_as::<Decodable>(
                    code_id, payload, gas_limit, value,
                )
            }
            (None, Some(_), Some(_)) => unreachable!(),
            (Some(_), None, None) => unreachable!(),
            (Some(_), None, Some(_)) => unreachable!(),
            (Some(_), Some(_), None) => unreachable!(),
            (Some(_), Some(_), Some(_)) => unreachable!(),
        }
    }
}
