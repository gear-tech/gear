// This file is part of Gear.

// Copyright (C) 2021-2025 Gear Technologies Inc.
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
use gear_common::{
    Origin,
    event::{CodeChangeKind, ProgramChangeKind},
};
use gear_core::ids::{ActorId, CodeId, MessageId};
use gear_utils::NonEmpty;
use gear_wasm_gen::wasm_gen_arbitrary::{Result, Unstructured};
use pallet_gear::Event as GearEvent;
use runtime_primitives::{AccountId, Balance};
use vara_runtime::{EXISTENTIAL_DEPOSIT, RuntimeEvent, System};

// Max code size - 25 KiB.
const MAX_CODE_SIZE: usize = 25 * 1024;

/// Maximum payload size for the fuzzer - 1 KiB.
///
/// TODO: #3442
const MAX_PAYLOAD_SIZE: usize = 1024;
const _: () = assert!(MAX_PAYLOAD_SIZE <= gear_core::buffer::MAX_PAYLOAD_SIZE);

/// Maximum salt size for the fuzzer - 512 bytes.
///
/// There's no need in large salts as we have only 35 extrinsics
/// for one run. Also small salt will make overall size of the
/// corpus smaller.
const MAX_SALT_SIZE: usize = 512;
const _: () = assert!(MAX_SALT_SIZE <= gear_core::buffer::MAX_PAYLOAD_SIZE);

const ID_SIZE: usize = size_of::<ActorId>();
const GAS_SIZE: usize = size_of::<u64>();
const VALUE_SIZE: usize = size_of::<u128>();

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
}

pub(crate) struct RuntimeStateViewProducer {
    corpus_id: String,
    sender: AccountId,
    programs: Option<NonEmpty<ActorId>>,
    codes: Option<NonEmpty<CodeId>>,
    // TODO #3703. Remove outdated message ids.
    mailbox: Option<NonEmpty<MessageId>>,
}

impl RuntimeStateViewProducer {
    pub(crate) fn new(corpus_id: String, sender: AccountId) -> Self {
        Self {
            corpus_id,
            sender,
            programs: None,
            codes: None,
            mailbox: None,
        }
    }

    pub(crate) fn produce_state_view(
        &mut self,
        balance_state: BalanceState,
    ) -> RuntimeStateView<'_> {
        self.update_state_view();

        RuntimeStateView {
            corpus_id: &self.corpus_id,
            current_balance: balance_state.into_inner(),
            programs: self.programs.as_ref(),
            codes: self.codes.as_ref(),
            mailbox: self.mailbox.as_ref(),
            max_gas: runtime::default_gas_limit(),
        }
    }

    /// Updates mailbox and existing programs view and resets events.
    fn update_state_view(&mut self) {
        let sender_program_id = self.sender.clone().cast();
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
                GearEvent::CodeChanged {
                    id,
                    change: CodeChangeKind::Active { .. },
                } => {
                    if let Some(codes) = self.codes.as_mut() {
                        codes.push(*id)
                    } else {
                        self.codes = Some(NonEmpty::new(*id));
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
    current_balance: Balance,
    corpus_id: &'a str,
    programs: Option<&'a NonEmpty<ActorId>>,
    codes: Option<&'a NonEmpty<CodeId>>,
    max_gas: u64,
    mailbox: Option<&'a NonEmpty<MessageId>>,
}

fn arbitrary_payload(u: &mut Unstructured) -> Result<Vec<u8>> {
    arbitrary_limited_bytes(u, MAX_PAYLOAD_SIZE)
}

fn arbitrary_limited_bytes(u: &mut Unstructured, limit: usize) -> Result<Vec<u8>> {
    let arb_size = u.int_in_range(0..=limit)?;
    u.bytes(arb_size).map(|bytes| bytes.to_vec())
}

fn arbitrary_value(u: &mut Unstructured, current_balance: u128) -> Result<u128> {
    let (lower, upper) = match u.int_in_range(0..=99)? {
        5..=10 => (0, 0),
        11..=30 => (
            EXISTENTIAL_DEPOSIT,
            (current_balance / 4).max(EXISTENTIAL_DEPOSIT),
        ),
        0..=2 => (0, EXISTENTIAL_DEPOSIT),
        _ => (EXISTENTIAL_DEPOSIT, current_balance),
    };

    u.int_in_range(lower..=upper)
}
