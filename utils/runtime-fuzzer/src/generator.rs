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

mod claim_value;
mod send_message;
mod send_reply;
mod upload_program;

use crate::{
    data::*,
    runtime::{self, BalanceState},
};
use gear_call_gen::GearCall;
use gear_common::{event::ProgramChangeKind, Origin};
use gear_core::ids::{MessageId, ProgramId};
use gear_utils::NonEmpty;
use gear_wasm_gen::wasm_gen_arbitrary::{Result, Unstructured};
use pallet_gear::Event as GearEvent;
use runtime_primitives::{AccountId, Balance};
use std::mem;
use vara_runtime::{RuntimeEvent, System};

// Max code size - 25 KiB.
const MAX_CODE_SIZE: usize = 25 * 1024;

/// Maximum payload size for the fuzzer - 1 KiB.
///
/// TODO: #3442
const MAX_PAYLOAD_SIZE: usize = 1024;
const _: () = assert!(MAX_PAYLOAD_SIZE <= gear_core::message::MAX_PAYLOAD_SIZE);

/// Maximum salt size for the fuzzer - 512 bytes.
///
/// There's no need in large salts as we have only 35 extrinsics
/// for one run. Also small salt will make overall size of the
/// corpus smaller.
const MAX_SALT_SIZE: usize = 512;
const _: () = assert!(MAX_SALT_SIZE <= gear_core::message::MAX_PAYLOAD_SIZE);

const ID_SIZE: usize = mem::size_of::<ProgramId>();
const GAS_SIZE: usize = mem::size_of::<u64>();
const VALUE_SIZE: usize = mem::size_of::<u128>();

/// Used to make sure that generators will not exceed `Unstructured` size as it's used not only
/// to generate things like wasm code or message payload but also to generate some auxiliary
/// data, for example index in some vec.
pub(crate) const AUXILIARY_SIZE: usize = 512;

pub(crate) struct GearCallsGenerator<'a> {
    unstructured: Unstructured<'a>,
    generated_upload_program: usize,
    generated_send_message: usize,
    generated_send_reply: usize,
    generated_claim_value: usize,
}

impl<'a> GearCallsGenerator<'a> {
    const UPLOAD_PROGRAM_CALL_ID: usize = 0;
    const SEND_MESSAGE_CALL_ID: usize = 1;
    const SEND_REPLY_CALL_ID: usize = 2;
    const CLAIM_VALUE_CALL_ID: usize = 3;

    pub(crate) fn new(data_requirement: FulfilledDataRequirement<'a, Self>) -> Self {
        Self {
            unstructured: Unstructured::new(data_requirement.data),
            generated_upload_program: 0,
            generated_send_message: 0,
            generated_send_reply: 0,
            generated_claim_value: 0,
        }
    }

    pub(crate) fn generate(&mut self, env: RuntimeStateView) -> Result<Option<GearCall>> {
        let call = if self.generated_upload_program < Self::MAX_UPLOAD_PROGRAM_CALLS {
            self.generated_upload_program += 1;

            upload_program::generate(&mut self.unstructured, env.into())
        } else if self.generated_send_message < Self::MAX_SEND_MESSAGE_CALLS {
            self.generated_send_message += 1;

            if env.programs.is_none() {
                upload_program::generate(&mut self.unstructured, env.into())
            } else {
                send_message::generate(
                    &mut self.unstructured,
                    env.try_into().expect("programs collection isn't empty"),
                )
            }
        } else if self.generated_send_reply < Self::MAX_SEND_REPLY_CALLS {
            self.generated_send_reply += 1;

            if env.mailbox.is_none() {
                upload_program::generate(&mut self.unstructured, env.into())
            } else {
                send_reply::generate(
                    &mut self.unstructured,
                    env.try_into().expect("mailbox isn't empty"),
                )
            }
        } else if self.generated_claim_value < Self::MAX_CLAIM_VALUE_CALLS {
            self.generated_claim_value += 1;

            if env.mailbox.is_none() {
                upload_program::generate(&mut self.unstructured, env.into())
            } else {
                claim_value::generate(
                    &mut self.unstructured,
                    env.try_into().expect("mailbox isn't empty"),
                )
            }
        } else {
            return Ok(None);
        };

        call.map(Some)
    }
}

impl GearCallsGenerator<'_> {
    // *WARNING*:
    //
    // Increasing these constants requires resetting minimal
    // size of fuzzer input buffer in corresponding scripts.
    pub(crate) const MAX_UPLOAD_PROGRAM_CALLS: usize = 10;
    pub(crate) const MAX_SEND_MESSAGE_CALLS: usize = 15;
    pub(crate) const MAX_SEND_REPLY_CALLS: usize = 1;
    pub(crate) const MAX_CLAIM_VALUE_CALLS: usize = 1;

    pub(crate) const fn random_data_requirement() -> usize {
        upload_program::data_requirement() * Self::MAX_UPLOAD_PROGRAM_CALLS
            + send_message::data_requirement() * Self::MAX_SEND_MESSAGE_CALLS
            + send_reply::data_requirement() * Self::MAX_SEND_REPLY_CALLS
            + claim_value::data_requirement() * Self::MAX_CLAIM_VALUE_CALLS
    }

    const fn send_reply_data_requirement() -> usize {
        ID_SIZE + MAX_PAYLOAD_SIZE + GAS_AND_VALUE_SIZE + AUXILIARY_SIZE
    }

    const fn claim_value_data_requirement() -> usize {
        ID_SIZE + AUXILIARY_SIZE
    }
}

pub(crate) struct RuntimeStateViewProducer {
    corpus_id: String,
    sender: AccountId,
    programs: Option<NonEmpty<ProgramId>>,
    // TODO #3703. Remove outdated message ids.
    mailbox: Option<NonEmpty<MessageId>>,
}

impl RuntimeStateViewProducer {
    pub(crate) fn new(corpus_id: String, sender: AccountId) -> Self {
        Self {
            corpus_id,
            sender,
            programs: None,
            mailbox: None,
        }
    }

    pub(crate) fn produce_state_view(&mut self, balance_state: BalanceState) -> RuntimeStateView {
        self.update_state_view();

        RuntimeStateView {
            corpus_id: &self.corpus_id,
            _current_balance: balance_state.into_inner(),
            programs: self.programs.as_ref(),
            mailbox: self.mailbox.as_ref(),
            max_gas: runtime::default_gas_limit(),
        }
    }

    /// Updates mailbox and existing programs view and resets events.
    fn update_state_view(&mut self) {
        let sender_program_id = ProgramId::from_origin(self.sender.clone().into_origin());
        System::events().iter().for_each(|e| {
            let RuntimeEvent::Gear(ref gear_event) = e.event else {
                return;
            };
            match gear_event {
                GearEvent::ProgramChanged {
                    id,
                    change: ProgramChangeKind::Active { .. },
                } => {
                    if let Some(programs) = self.programs.as_mut() {
                        programs.push(*id)
                    } else {
                        self.programs = Some(NonEmpty::new(*id));
                    }
                }
                GearEvent::UserMessageSent {
                    message,
                    expiration: Some(_),
                } => {
                    if message.destination() == sender_program_id {
                        if let Some(mailbox) = self.mailbox.as_mut() {
                            mailbox.push(message.id())
                        } else {
                            self.mailbox = Some(NonEmpty::new(message.id()));
                        }
                    }
                }
                _ => {}
            }
        });

        // Resetting events brings 2 benefits:
        // 1. We do not iterate over same events from block to block.
        // 2. Obtained mailbox message ids and initialized programs ids are unique.
        System::reset_events();
    }
}

pub(crate) struct RuntimeStateView<'a> {
    corpus_id: &'a str,
    _current_balance: Balance,
    programs: Option<&'a NonEmpty<ProgramId>>,
    max_gas: u64,
    mailbox: Option<&'a NonEmpty<MessageId>>,
}

// todo - is it a good design?
pub(crate) struct RuntimeInterimState {
    programs: HashSet<ProgramId>,
    // todo issue - include time limits, so no outdated mailbox messages will be stored.
    mailbox: HashSet<MessageId>,
}

impl RuntimeInterimState {
    pub(crate) fn build() -> Self {
        let mut programs = HashSet::new();
        let mut mailbox = HashSet::new();
        System::events().iter().for_each(|e| {
            let RuntimeEvent::Gear(ref gear_event) = e.event else {
                return;
            };
            match gear_event {
                GearEvent::ProgramChanged {
                    id,
                    change: ProgramChangeKind::Active { .. },
                } => {
                    programs.insert(*id);
                }
                GearEvent::UserMessageSent {
                    message,
                    expiration: Some(_),
                } => {
                    if message.destination() == runtime::alice_program_id() {
                        mailbox.insert(message.id());
                    }
                }
                _ => {}
            }
        });
        System::reset_events();

        Self { programs, mailbox }
    }

    fn merge(&mut self, Self { programs, mailbox }: Self) {
        self.programs.extend(programs);
        self.mailbox.extend(mailbox);
    }
}

pub(crate) struct GenerationEnvironment<'a> {
    corpus_id: &'a str,
    programs: Vec<&'a ProgramId>,
    max_gas: u64,
    mailbox: Vec<&'a MessageId>,
}

impl<'a> From<GenerationEnvironment<'a>> for UploadProgramRuntimeData<'a> {
    fn from(env: GenerationEnvironment<'a>) -> Self {
        (env.corpus_id, env.programs, env.max_gas)
    }
}

impl<'a> TryFrom<GenerationEnvironment<'a>> for SendMessageRuntimeData<'a> {
    type Error = ();

    fn try_from(env: GenerationEnvironment<'a>) -> StdResult<Self, Self::Error> {
        let programs = NonEmpty::from_slice(&env.programs).ok_or(())?;

        Ok((programs, env.max_gas))
    }
}

impl<'a> TryFrom<GenerationEnvironment<'a>> for SendReplyRuntimeData<'a> {
    type Error = ();

    fn try_from(env: GenerationEnvironment<'a>) -> StdResult<Self, Self::Error> {
        let mailbox = NonEmpty::from_slice(&env.mailbox).ok_or(())?;

        Ok((mailbox, env.max_gas))
    }
}

impl<'a> TryFrom<GenerationEnvironment<'a>> for ClaimValueRuntimeData<'a> {
    type Error = ();

    fn try_from(env: GenerationEnvironment<'a>) -> StdResult<Self, Self::Error> {
        NonEmpty::from_slice(&env.mailbox)
            .map(|mailbox| (mailbox,))
            .ok_or(())
    }
}

fn arbitrary_payload(u: &mut Unstructured) -> Result<Vec<u8>> {
    arbitrary_limited_bytes(u, MAX_PAYLOAD_SIZE)
}

fn arbitrary_limited_bytes(u: &mut Unstructured, limit: usize) -> Result<Vec<u8>> {
    let arb_size = u.int_in_range(0..=limit)?;
    u.bytes(arb_size).map(|bytes| bytes.to_vec())
}
