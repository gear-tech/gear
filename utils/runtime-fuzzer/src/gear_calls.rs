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

//! [`GearCalls`] implementation.
//!
//! [`GearCalls`]'s purpose is to lazy evaluate [`GearCall`]s using provided [`Config`]
//! (see the [`GearCalls`] docs for the more detailed information about usage).
//!
//! [`Config`] is basically a set of [`ExtrinsicGenerator`]s, each of them is capable of
//! generating one particular extrinsic's parameters, for example - [`UploadProgramGenerator`]
//! or [`SendMessageGenerator`].
//!
//! Some of the extrinsics require access to the mailbox messages, such as `SendReply`
//! and `ClaimValue` ones. This access is provided by the [`MailboxProvider`] trait
//! which must be implemented by the callling side and passed into generators which require it.

use crate::{GearCall, SendMessageArgs, UploadProgramArgs};
use arbitrary::{Error, Result, Unstructured};
use gear_call_gen::{ClaimValueArgs, SendReplyArgs};
use gear_core::ids::{CodeId, MessageId, ProgramId};
use gear_utils::NonEmpty;
use gear_wasm_gen::{
    EntryPointsSet, InvocableSysCall, ParamType, StandardGearWasmConfigsBundle, SysCallName,
    SysCallsInjectionTypes, SysCallsParamsConfig,
};
use std::mem;

/// Maximum payload size for the fuzzer - 1 KiB.
///
/// TODO: #3442
const MAX_PAYLOAD_SIZE: usize = 1024;
static_assertions::const_assert!(MAX_PAYLOAD_SIZE <= gear_core::message::MAX_PAYLOAD_SIZE);

/// Maximum salt size for the fuzzer - 512 bytes.
///
/// There's no need in large salts as we have only 35 extrinsics
/// for one run. Also small salt will make overall size of the
/// corpus smaller.
const MAX_SALT_SIZE: usize = 512;
static_assertions::const_assert!(MAX_SALT_SIZE <= gear_core::message::MAX_PAYLOAD_SIZE);

const ID_SIZE: usize = mem::size_of::<ProgramId>();
const GAS_AND_VALUE_SIZE: usize = mem::size_of::<(u64, u128)>();
// Used to make sure that generators will not exceed `Unstructured` size as it's used not only
// to generate things like wasm code or message payload but also to generate some auxiliary
// data, for example index in some vec.
const AUXILIARY_SIZE: usize = 512;

/// This trait provides ability for [`ExtrinsicGenerator`]s to fetch messages
/// from mailbox, for example [`UploadProgramGenerator`] and
/// [`ClaimValueGenerator`] use it.
pub(crate) trait MailboxProvider {
    fn fetch_messages(&self) -> Vec<MessageId>;
}

/// Struct that's used by `GearCalls` to store some data available only in the
/// middle of calls generation such as uploaded program ids.
///
/// ### Note
/// This struct shouldn't be used directly, as it should be updated and read
/// only by the `GearCalls`.
pub(crate) struct TempData {
    existing_addresses: Vec<ProgramId>,
}

/// The struct for generating gear extrinsics.
///
/// # Usage
///
/// `Iterator<Item = GearCall>` is implemented for this struct, so for
/// generating gear calls you need to iterate over [`GearCalls`].
pub(crate) struct GearCalls<'a> {
    unstructured: Unstructured<'a>,
    intermediate_data: TempData,

    generators: Vec<RepeatedGenerator>,

    current_generator: usize,
    current_extrinsic: usize,
}

impl<'a> GearCalls<'a> {
    pub(crate) fn new(
        data: &'a [u8],
        generators: ExtrinsicGeneratorSet,
        existing_users: Vec<ProgramId>,
    ) -> Result<GearCalls<'a>> {
        if data.len() < generators.unstructured_size_hint() {
            return Err(Error::NotEnoughData);
        }

        Ok(GearCalls {
            unstructured: Unstructured::new(data),
            intermediate_data: TempData {
                existing_addresses: existing_users,
            },
            current_generator: 0,
            current_extrinsic: 0,
            generators: generators.0,
        })
    }
}

impl Iterator for GearCalls<'_> {
    type Item = Result<GearCall>;

    fn next(&mut self) -> Option<Result<GearCall>> {
        loop {
            if self.current_generator == self.generators.len() {
                return None;
            }

            let generator = &self.generators[self.current_generator].generator;
            let call = generator.generate(&mut self.intermediate_data, &mut self.unstructured);

            self.current_extrinsic += 1;
            if self.current_extrinsic == self.generators[self.current_generator].amount {
                self.current_extrinsic = 0;
                self.current_generator += 1;
            }

            return match call {
                Ok(Some(call)) => Some(Ok(call)),
                Ok(None) => continue,
                Err(err) => Some(Err(err)),
            };
        }
    }
}

/// Extrinsic generators that will be used inside [`GearCalls`] to generate all the calls needed.
pub(crate) struct ExtrinsicGeneratorSet(Vec<RepeatedGenerator>);

impl ExtrinsicGeneratorSet {
    pub(crate) fn new(generators: Vec<RepeatedGenerator>) -> ExtrinsicGeneratorSet {
        ExtrinsicGeneratorSet(generators)
    }

    pub(crate) fn unstructured_size_hint(&self) -> usize {
        self.0
            .iter()
            .map(|generator| generator.amount * generator.generator.unstructured_size_hint())
            .sum()
    }
}

/// [`ExtrinsicGenerator`] that should be invoked [`RepeatedGenerator::amount`](`amount`) times
/// by the [`GearCalls`] to generate [`RepeatedGenerator::amount`](`amount`) extrinsics.
pub(crate) struct RepeatedGenerator {
    pub amount: usize,
    pub generator: ExtrinsicGenerator,
}

impl RepeatedGenerator {
    pub fn new(amount: usize, generator: ExtrinsicGenerator) -> RepeatedGenerator {
        RepeatedGenerator { amount, generator }
    }
}

/// Enum containing all the possible concrete extrinsic generators.
pub(crate) enum ExtrinsicGenerator {
    UploadProgram(UploadProgramGenerator),
    SendMessage(SendMessageGenerator),
    SendReply(SendReplyGenerator),
    ClaimValue(ClaimValueGenerator),
}

impl ExtrinsicGenerator {
    pub(crate) fn generate(
        &self,
        intermediate_data: &mut TempData,
        unstructured: &mut Unstructured,
    ) -> Result<Option<GearCall>> {
        match self {
            Self::UploadProgram(g) => g.generate(intermediate_data, unstructured),
            Self::SendMessage(g) => g.generate(intermediate_data, unstructured),
            Self::SendReply(g) => g.generate(unstructured),
            Self::ClaimValue(g) => g.generate(unstructured),
        }
    }

    pub(crate) const fn unstructured_size_hint(&self) -> usize {
        match self {
            Self::UploadProgram(g) => g.unstructured_size_hint(),
            Self::SendMessage(g) => g.unstructured_size_hint(),
            Self::SendReply(g) => g.unstructured_size_hint(),
            Self::ClaimValue(g) => g.unstructured_size_hint(),
        }
    }
}

/// Extrinsic generator that's capable of generating `UploadProgram` calls.
pub(crate) struct UploadProgramGenerator {
    pub gas: u64,
    pub value: u128,
    pub test_input_id: String,
}

impl UploadProgramGenerator {
    fn generate(
        &self,
        intermediate_data: &mut TempData,
        unstructured: &mut Unstructured,
    ) -> Result<Option<GearCall>> {
        log::trace!("New gear-wasm generation");
        log::trace!("Random data before wasm gen {}", unstructured.len());

        let code = gear_wasm_gen::generate_gear_program_code(
            unstructured,
            config(
                &intermediate_data.existing_addresses,
                Some(format!(
                    "Generated program from corpus - {}",
                    &self.test_input_id
                )),
            ),
        )?;
        log::trace!("Random data after wasm gen {}", unstructured.len());
        log::trace!("Code length {:?}", code.len());

        let salt = arbitrary_salt(unstructured)?;
        log::trace!("Random data after salt gen {}", unstructured.len());
        log::trace!("Salt length {:?}", salt.len());

        let payload = arbitrary_payload(unstructured)?;
        log::trace!(
            "Random data after payload (upload_program) gen {}",
            unstructured.len()
        );
        log::trace!("Payload (upload_program) length {:?}", payload.len());

        let program_id = ProgramId::generate_from_user(CodeId::generate(&code), &salt);

        log::trace!("Generated code for program id - {program_id}");

        intermediate_data.existing_addresses.push(program_id);

        Ok(Some(
            UploadProgramArgs((code, salt, payload, self.gas, self.value)).into(),
        ))
    }

    const fn unstructured_size_hint(&self) -> usize {
        // Max code size - 25 KiB.
        const MAX_CODE_SIZE: usize = 25 * 1024;

        MAX_CODE_SIZE + MAX_SALT_SIZE + MAX_PAYLOAD_SIZE + GAS_AND_VALUE_SIZE + AUXILIARY_SIZE
    }
}

impl From<UploadProgramGenerator> for ExtrinsicGenerator {
    fn from(g: UploadProgramGenerator) -> ExtrinsicGenerator {
        ExtrinsicGenerator::UploadProgram(g)
    }
}

/// Extrinsic generator that's capable of generating `SendMessage` calls.
pub(crate) struct SendMessageGenerator {
    pub gas: u64,
    pub value: u128,
}

impl SendMessageGenerator {
    fn generate(
        &self,
        intermediate_data: &mut TempData,
        unstructured: &mut Unstructured,
    ) -> Result<Option<GearCall>> {
        let program_id = unstructured
            .choose(&intermediate_data.existing_addresses)
            .copied()?;
        let payload = arbitrary_payload(unstructured)?;
        log::trace!(
            "Random data after payload (send_message) gen {}",
            unstructured.len()
        );
        log::trace!("Payload (send_message) length {:?}", payload.len());

        Ok(Some(
            SendMessageArgs((program_id, payload, self.gas, self.value)).into(),
        ))
    }

    const fn unstructured_size_hint(&self) -> usize {
        ID_SIZE + MAX_PAYLOAD_SIZE + GAS_AND_VALUE_SIZE + AUXILIARY_SIZE
    }
}

impl From<SendMessageGenerator> for ExtrinsicGenerator {
    fn from(g: SendMessageGenerator) -> ExtrinsicGenerator {
        ExtrinsicGenerator::SendMessage(g)
    }
}

/// Extrinsic generator that's capable of generating `SendReply` calls.
pub(crate) struct SendReplyGenerator {
    pub mailbox_provider: Box<dyn MailboxProvider>,

    pub gas: u64,
    pub value: u128,
}

impl SendReplyGenerator {
    fn generate(&self, unstructured: &mut Unstructured) -> Result<Option<GearCall>> {
        log::trace!(
            "Random data before payload (send_reply) gen {}",
            unstructured.len()
        );
        let message_id =
            arbitrary_message_id_from_mailbox(unstructured, self.mailbox_provider.as_ref())?;

        Ok(match message_id {
            None => None,
            Some(message_id) => {
                let payload = arbitrary_payload(unstructured)?;
                log::trace!(
                    "Random data after payload (send_reply) gen {}",
                    unstructured.len()
                );
                log::trace!("Payload (send_reply) length {:?}", payload.len());

                Some(SendReplyArgs((message_id, payload, self.gas, self.value)).into())
            }
        })
    }

    const fn unstructured_size_hint(&self) -> usize {
        ID_SIZE + MAX_PAYLOAD_SIZE + GAS_AND_VALUE_SIZE + AUXILIARY_SIZE
    }
}

impl From<SendReplyGenerator> for ExtrinsicGenerator {
    fn from(g: SendReplyGenerator) -> ExtrinsicGenerator {
        ExtrinsicGenerator::SendReply(g)
    }
}

/// Extrinsic generator that's capable of generating `ClaimValue` calls.
pub(crate) struct ClaimValueGenerator {
    pub mailbox_provider: Box<dyn MailboxProvider>,
}

impl ClaimValueGenerator {
    fn generate(&self, unstructured: &mut Unstructured) -> Result<Option<GearCall>> {
        log::trace!("Generating claim_value call");
        let message_id =
            arbitrary_message_id_from_mailbox(unstructured, self.mailbox_provider.as_ref())?;
        Ok(message_id.map(|msg_id| ClaimValueArgs(msg_id).into()))
    }

    const fn unstructured_size_hint(&self) -> usize {
        ID_SIZE + AUXILIARY_SIZE
    }
}

impl From<ClaimValueGenerator> for ExtrinsicGenerator {
    fn from(g: ClaimValueGenerator) -> ExtrinsicGenerator {
        ExtrinsicGenerator::ClaimValue(g)
    }
}

fn arbitrary_message_id_from_mailbox(
    u: &mut Unstructured,
    mailbox_provider: &dyn MailboxProvider,
) -> Result<Option<MessageId>> {
    let messages = mailbox_provider.fetch_messages();

    if messages.is_empty() {
        log::trace!("Mailbox is empty.");
        Ok(None)
    } else {
        log::trace!("Mailbox is not empty, len = {}", messages.len());
        u.choose(&messages).cloned().map(Some)
    }
}

fn arbitrary_salt(u: &mut Unstructured) -> Result<Vec<u8>> {
    arbitrary_limited_bytes(u, MAX_SALT_SIZE)
}

fn arbitrary_payload(u: &mut Unstructured) -> Result<Vec<u8>> {
    arbitrary_limited_bytes(u, MAX_PAYLOAD_SIZE)
}

fn arbitrary_limited_bytes(u: &mut Unstructured, limit: usize) -> Result<Vec<u8>> {
    let arb_size = u.int_in_range(0..=limit)?;
    u.bytes(arb_size).map(|bytes| bytes.to_vec())
}

fn config(
    programs: &[ProgramId],
    log_info: Option<String>,
) -> StandardGearWasmConfigsBundle<ProgramId> {
    let initial_pages = 2;
    let mut injection_types = SysCallsInjectionTypes::all_once();
    injection_types.set_multiple(
        [
            (SysCallName::Leave, 0..=0),
            (SysCallName::Panic, 0..=0),
            (SysCallName::OomPanic, 0..=0),
            (SysCallName::EnvVars, 0..=0),
            (SysCallName::Send, 10..=15),
            (SysCallName::Exit, 0..=1),
            (SysCallName::Alloc, 3..=6),
            (SysCallName::Free, 3..=6),
        ]
        .map(|(syscall, range)| (InvocableSysCall::Loose(syscall), range))
        .into_iter(),
    );

    let mut params_config = SysCallsParamsConfig::default();
    params_config.add_rule(ParamType::Alloc, (10..=20).into());
    params_config.add_rule(ParamType::Free, (initial_pages..=initial_pages + 35).into());

    let existing_addresses = NonEmpty::collect(
        programs
            .iter()
            .copied()
            .filter(|&pid| pid != ProgramId::default()),
    );

    log::trace!(
        "Messages will be sent to existing addresses {:?}",
        existing_addresses
    );

    StandardGearWasmConfigsBundle {
        entry_points_set: EntryPointsSet::InitHandleHandleReply,
        injection_types,
        existing_addresses,
        log_info,
        params_config,
        initial_pages: initial_pages as u32,
        ..Default::default()
    }
}
