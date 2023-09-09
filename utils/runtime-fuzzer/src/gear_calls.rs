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

use crate::{runtime::default_gas_limit, GearCall, SendMessageArgs, UploadProgramArgs};
use arbitrary::{Error, Result, Unstructured};
use gear_call_gen::{ClaimValueArgs, SendReplyArgs};
use gear_core::ids::{CodeId, MessageId, ProgramId};
use gear_utils::NonEmpty;
use gear_wasm_gen::{
    EntryPointsSet, StandardGearWasmConfigsBundle, SysCallName, SysCallsInjectionAmounts,
};
use sha1::*;
use std::rc::Rc;

/// Maximum payload size for the fuzzer - 512 KiB.
const MAX_PAYLOAD_SIZE: usize = 512 * 1024;
static_assertions::const_assert!(MAX_PAYLOAD_SIZE <= gear_core::message::MAX_PAYLOAD_SIZE);

/// The struct for generating gear extrinsics.
///
/// # Usage
///
/// `Iterator<Item = GearCall>` is implemented for this struct, so for
/// generating gear calls you need to iterate over [`GearCalls`].
pub struct GearCalls<'a> {
    unstructured: Unstructured<'a>,
    intermediate_data: IntermediateData,

    config: ExtrinsicsConfig,

    current_generator: usize,
    current_extrinsic: usize,
}

impl<'a> GearCalls<'a> {
    pub fn new(
        data: &'a [u8],
        mailbox_provider: Rc<Box<dyn MailboxProvider>>,
    ) -> Result<GearCalls<'a>> {
        log::trace!(
            "New GearCalls generation: random data received {}",
            data.len()
        );
        let test_input_id = get_sha1_string(data);
        log::trace!("Generating GearCalls from corpus - {}", test_input_id);

        let config = Self::default_config(mailbox_provider, test_input_id);

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

    pub fn unstructured_size_hint() -> usize {
        Self::default_config(
            Rc::from(Box::from(MockMailboxProvider) as Box<dyn MailboxProvider>),
            "".to_string(),
        )
        .unstructured_size_hint()
    }

    fn default_config(
        mailbox_provider: Rc<Box<dyn MailboxProvider>>,
        test_input_id: String,
    ) -> ExtrinsicsConfig {
        ExtrinsicsConfig {
            generators: vec![
                (
                    10,
                    Box::new(UploadProgramGenerator {
                        gas: default_gas_limit(),
                        value: 0,
                        test_input_id,
                    }),
                ),
                (
                    15,
                    Box::new(SendMessageGenerator {
                        gas: default_gas_limit(),
                        value: 0,
                        prepaid: false,
                    }),
                ),
                (
                    1,
                    Box::new(SendReplyGenerator {
                        mailbox_provider,
                        gas: default_gas_limit(),
                        value: 0,
                        prepaid: false,
                    }),
                ),
            ],
        }
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

pub trait MailboxProvider {
    fn fetch_messages(&self) -> Vec<MessageId>;
}

#[derive(Clone)]
struct MockMailboxProvider;

impl MailboxProvider for MockMailboxProvider {
    fn fetch_messages(&self) -> Vec<MessageId> {
        unimplemented!()
    }
}

/// Data that is persistent between different [`ExtrinsicGenerator`]s calls.
#[derive(Default)]
struct IntermediateData {
    uploaded_programs: Vec<ProgramId>,
}

type ExtrinsicAmount = usize;

struct ExtrinsicsConfig {
    generators: Vec<(ExtrinsicAmount, Box<dyn ExtrinsicGenerator>)>,
}

impl ExtrinsicsConfig {
    fn unstructured_size_hint(&self) -> usize {
        self.generators
            .iter()
            .map(|(amount, generator)| amount * generator.unstructured_size_hint())
            .sum()
    }
}

trait ExtrinsicGenerator {
    fn generate(
        &self,
        intermediate_data: &mut IntermediateData,
        unstructured: &mut Unstructured,
    ) -> Result<GearCall>;

    fn unstructured_size_hint(&self) -> usize;
}

struct UploadProgramGenerator {
    gas: u64,
    value: u128,
    test_input_id: String,
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

struct SendMessageGenerator {
    gas: u64,
    value: u128,
    prepaid: bool,
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

struct SendReplyGenerator {
    mailbox_provider: Rc<Box<dyn MailboxProvider>>,

    gas: u64,
    value: u128,
    prepaid: bool,
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
            arbitrary_message_id_from_mailbox(unstructured, self.mailbox_provider.clone())?;

        let payload = arbitrary_payload(unstructured)?;
        log::error!(
            "Random data after payload (send_reply) gen {}",
            unstructured.len()
        );
        log::trace!("Payload (send_reply) length {:?}", payload.len());

        log::error!("\n");

        Ok(SendReplyArgs((message_id, payload, self.gas, self.value, self.prepaid)).into())
    }

    fn unstructured_size_hint(&self) -> usize {
        // 512 KiB for payload.
        520 * 1024
    }
}

struct ClaimValueGenerator {
    mailbox_provider: Rc<Box<dyn MailboxProvider>>,
}

impl ExtrinsicGenerator for ClaimValueGenerator {
    fn generate(
        &self,
        _: &mut IntermediateData,
        unstructured: &mut Unstructured,
    ) -> Result<GearCall> {
        log::trace!("Generating claim_value call");
        let message_id =
            arbitrary_message_id_from_mailbox(unstructured, self.mailbox_provider.clone())?;

        Ok(ClaimValueArgs(message_id).into())
    }

    fn unstructured_size_hint(&self) -> usize {
        // 32 bytes for message id.
        100
    }
}

fn arbitrary_message_id_from_mailbox(
    u: &mut Unstructured,
    mailbox_provider: Rc<Box<dyn MailboxProvider>>,
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
    let mut injection_amounts = SysCallsInjectionAmounts::all_never();
    injection_amounts.set(SysCallName::Leave, 0, 0);
    injection_amounts.set(SysCallName::Panic, 0, 0);
    injection_amounts.set(SysCallName::OomPanic, 0, 0);
    injection_amounts.set(SysCallName::Send, 20, 30);
    injection_amounts.set(SysCallName::Exit, 0, 1);

    let existing_addresses = NonEmpty::collect(programs.iter().copied());

    log::trace!(
        "Messages will be sent to existing addresses {:?}",
        existing_addresses
    );

    StandardGearWasmConfigsBundle {
        entry_points_set: EntryPointsSet::InitHandleHandleReply,
        injection_amounts,
        existing_addresses,
        log_info,

        ..Default::default()
    }
}

fn get_sha1_string(input: &[u8]) -> String {
    let mut hasher = sha1::Sha1::new();
    hasher.update(input);

    hex::encode(hasher.finalize())
}
