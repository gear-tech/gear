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

//! `GearCalls` implementation.

use crate::{GearCall, SendMessageArgs, UploadProgramArgs};
use arbitrary::{Error, Result, Unstructured};
use gear_call_gen::{ClaimValueArgs, SendReplyArgs};
use gear_core::ids::{CodeId, MessageId, ProgramId};
use gear_utils::NonEmpty;
use gear_wasm_gen::{
    EntryPointsSet, ParamType, StandardGearWasmConfigsBundle, SysCallName,
    SysCallsInjectionAmounts, SysCallsParamsConfig,
};

/// Maximum payload size for the fuzzer - 512 KiB.
const MAX_PAYLOAD_SIZE: usize = 512 * 1024;
static_assertions::const_assert!(MAX_PAYLOAD_SIZE <= gear_core::message::MAX_PAYLOAD_SIZE);

/// The struct for generating gear extrinsics.
///
/// # Usage
///
/// `Iterator<Item = GearCall>` is implemented for this struct, so for
/// generating gear calls you need to iterate over [`GearCalls`].
pub(crate) struct GearCalls<'a> {
    unstructured: Unstructured<'a>,
    intermediate_data: IntermediateData,

    config: Config,

    current_generator: usize,
    current_extrinsic: usize,
}

impl<'a> GearCalls<'a> {
    pub fn new(data: &'a [u8], config: Config) -> Result<GearCalls<'a>> {
        if data.len() < config.unstructured_size_hint() {
            return Err(Error::NotEnoughData);
        }

        Ok(GearCalls {
            unstructured: Unstructured::new(data),
            intermediate_data: IntermediateData::default(),
            current_generator: 0,
            current_extrinsic: 0,
            config,
        })
    }
}

impl Iterator for GearCalls<'_> {
    type Item = Result<GearCall>;

    fn next(&mut self) -> Option<Result<GearCall>> {
        if self.current_generator == self.config.generators.len() {
            return None;
        }

        let (_, generator) = &self.config.generators[self.current_generator];
        let call = generator.generate(&mut self.intermediate_data, &mut self.unstructured);

        self.current_extrinsic += 1;
        if self.current_extrinsic == self.config.generators[self.current_generator].0 {
            self.current_extrinsic = 0;
            self.current_generator += 1;
        }

        Some(call)
    }
}

pub(crate) type ExtrinsicAmount = usize;

/// Config that's used in the process of generation gear calls.
pub(crate) struct Config {
    generators: Vec<(ExtrinsicAmount, Box<dyn ExtrinsicGenerator>)>,
}

impl Config {
    pub fn new(generators: Vec<(ExtrinsicAmount, Box<dyn ExtrinsicGenerator>)>) -> Config {
        Config { generators }
    }

    pub(crate) fn unstructured_size_hint(&self) -> usize {
        self.generators
            .iter()
            .map(|(amount, generator)| amount * generator.unstructured_size_hint())
            .sum()
    }
}

/// Type that's expected by some generators in order to fetch mailbox messages.
pub(crate) trait MailboxProvider {
    fn fetch_messages(&self) -> Vec<MessageId>;
}

pub(crate) trait ExtrinsicGenerator {
    fn generate(
        &self,
        intermediate_data: &mut IntermediateData,
        unstructured: &mut Unstructured,
    ) -> Result<GearCall>;

    fn unstructured_size_hint(&self) -> usize;
}

/// Data that is persistent between different [`ExtrinsicGenerator`]s calls.
#[derive(Default)]
pub(crate) struct IntermediateData {
    uploaded_programs: Vec<ProgramId>,
}

/// Extrinsic generator that's capable of generationg `UploadProgram` calls.
pub(crate) struct UploadProgramGenerator {
    pub gas: u64,
    pub value: u128,
    pub test_input_id: String,
}

impl ExtrinsicGenerator for UploadProgramGenerator {
    fn generate(
        &self,
        intermediate_data: &mut IntermediateData,
        unstructured: &mut Unstructured,
    ) -> Result<GearCall> {
        log::trace!("New gear-wasm generation");
        log::trace!("Random data before wasm gen {}", unstructured.len());

        let code = gear_wasm_gen::generate_gear_program_code(
            unstructured,
            config(
                &intermediate_data.uploaded_programs,
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

        let program_id = ProgramId::generate(CodeId::generate(&code), &salt);

        log::trace!("Generated code for program id - {program_id}");

        intermediate_data.uploaded_programs.push(program_id);

        Ok(UploadProgramArgs((code, salt, payload, self.gas, self.value)).into())
    }

    fn unstructured_size_hint(&self) -> usize {
        // 1024 KiB for payload and salt and 50 KiB for code.
        1080 * 1024
    }
}

/// Extrinsic generator that's capable of generating `SendMessage` calls.
pub(crate) struct SendMessageGenerator {
    pub gas: u64,
    pub value: u128,
    pub prepaid: bool,
}

impl ExtrinsicGenerator for SendMessageGenerator {
    fn generate(
        &self,
        intermediate_data: &mut IntermediateData,
        unstructured: &mut Unstructured,
    ) -> Result<GearCall> {
        let program_id = unstructured
            .choose(&intermediate_data.uploaded_programs)
            .copied()?;
        let payload = arbitrary_payload(unstructured)?;
        log::trace!(
            "Random data after payload (send_message) gen {}",
            unstructured.len()
        );
        log::trace!("Payload (send_message) length {:?}", payload.len());

        Ok(SendMessageArgs((program_id, payload, self.gas, self.value, self.prepaid)).into())
    }

    fn unstructured_size_hint(&self) -> usize {
        // 512 KiB for payload.
        520 * 1024
    }
}

/// Extrinsic generator that's capable of generating `SendReply` calls.
pub(crate) struct SendReplyGenerator {
    pub mailbox_provider: Box<dyn MailboxProvider>,

    pub gas: u64,
    pub value: u128,
    pub prepaid: bool,
}

impl ExtrinsicGenerator for SendReplyGenerator {
    fn generate(
        &self,
        _: &mut IntermediateData,
        unstructured: &mut Unstructured,
    ) -> Result<GearCall> {
        log::trace!(
            "Random data before payload (send_reply) gen {}",
            unstructured.len()
        );
        let message_id =
            arbitrary_message_id_from_mailbox(unstructured, self.mailbox_provider.as_ref())?;

        let payload = arbitrary_payload(unstructured)?;
        log::trace!(
            "Random data after payload (send_reply) gen {}",
            unstructured.len()
        );
        log::trace!("Payload (send_reply) length {:?}", payload.len());

        Ok(SendReplyArgs((message_id, payload, self.gas, self.value, self.prepaid)).into())
    }

    fn unstructured_size_hint(&self) -> usize {
        // 512 KiB for payload.
        520 * 1024
    }
}

/// Extrinsic generator that's capable of generating `ClaimValue` calls.
pub(crate) struct ClaimValueGenerator {
    pub mailbox_provider: Box<dyn MailboxProvider>,
}

impl ExtrinsicGenerator for ClaimValueGenerator {
    fn generate(
        &self,
        _: &mut IntermediateData,
        unstructured: &mut Unstructured,
    ) -> Result<GearCall> {
        log::trace!("Generating claim_value call");
        let message_id =
            arbitrary_message_id_from_mailbox(unstructured, self.mailbox_provider.as_ref())?;

        Ok(ClaimValueArgs(message_id).into())
    }

    fn unstructured_size_hint(&self) -> usize {
        // 32 bytes for message id.
        100
    }
}

fn arbitrary_message_id_from_mailbox(
    u: &mut Unstructured,
    mailbox_provider: &dyn MailboxProvider,
) -> Result<MessageId> {
    let messages = mailbox_provider.fetch_messages();

    if messages.is_empty() {
        log::trace!("Mailbox is empty. Selecting random message id");
        arbitrary_message_id(u)
    } else {
        log::trace!("Mailbox is not empty, len = {}", messages.len());
        u.choose(&messages).cloned()
    }
}

fn arbitrary_message_id(u: &mut Unstructured) -> Result<MessageId> {
    let mut data = [0; 32];
    u.fill_buffer(&mut data)?;
    Ok(MessageId::from(&data[..]))
}

fn arbitrary_salt(u: &mut Unstructured) -> Result<Vec<u8>> {
    arbitrary_limited_bytes(u, MAX_PAYLOAD_SIZE)
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
    let mut injection_amounts = SysCallsInjectionAmounts::all_once();
    injection_amounts.set_multiple(
        [
            (SysCallName::Leave, 0..=0),
            (SysCallName::Panic, 0..=0),
            (SysCallName::OomPanic, 0..=0),
            (SysCallName::Send, 20..=30),
            (SysCallName::Exit, 0..=1),
            (SysCallName::Alloc, 20..=30),
            (SysCallName::Free, 20..=30),
        ]
        .into_iter(),
    );

    let mut params_config = SysCallsParamsConfig::default();
    params_config.add_rule(ParamType::Alloc, (10..=20).into());
    params_config.add_rule(
        ParamType::Free,
        (initial_pages..=initial_pages + 250).into(),
    );

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
        injection_amounts,
        existing_addresses,
        log_info,
        params_config,
        initial_pages: initial_pages as u32,
        ..Default::default()
    }
}
