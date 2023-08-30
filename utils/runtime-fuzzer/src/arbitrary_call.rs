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

//! `Arbitrary` trait implementation for a collection of [`RawGearCall`].

use crate::{runtime::default_gas_limit, GearCall, SendMessageArgs, UploadProgramArgs};
use arbitrary::{Arbitrary, Result, Unstructured};
use gear_call_gen::{ClaimValueArgs, SendReplyArgs};
use gear_core::ids::{CodeId, MessageId, ProgramId};
use gear_utils::NonEmpty;
use gear_wasm_gen::{
    EntryPointsSet, StandardGearWasmConfigsBundle, SysCallName, SysCallsInjectionAmounts,
};
use sha1::*;
use std::{
    fmt::Debug,
    mem::{self, MaybeUninit},
};

/// Maximum payload size for the fuzzer - 512 KiB.
const MAX_PAYLOAD_SIZE: usize = 512 * 1024;
static_assertions::const_assert!(MAX_PAYLOAD_SIZE <= gear_core::message::MAX_PAYLOAD_SIZE);

/// Newtype for [`GearCall`] that's not fully initialized and
/// requires some data fetched in the process of fuzzing to completely initialize.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RawGearCall(GearCall);

/// New-type wrapper over array of [`RawGearCall`]s.
///
/// It's main purpose is to be an implementor of `Arbitrary` for the array of [`RawGearCall`]s.
/// New-type is required as array is always a foreign type.
#[derive(Clone, PartialEq, Eq)]
pub struct GearCalls(pub [RawGearCall; GearCalls::MAX_CALLS]);

/// That's done because when fuzzer finds a crash it prints a [`Debug`] string of the [`GearCalls`].
/// Fuzzer executes [`GearCalls`] with pretty large codes and payloads, therefore to avoid printing huge
/// amount of data we do a mock implementation of [`Debug`].
///
/// If one wants to see a real debug string of the type, a separate wrapper over [`RawGearCall`]s array must be
/// implemented. This wrapper must implement [`Debug`] then.
impl Debug for GearCalls {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_tuple("GearCalls")
            .field(&"Mock `Debug` impl")
            .finish()
    }
}

impl GearCalls {
    pub const MAX_CALLS: usize = 35;
}

impl Arbitrary for GearCalls {
    fn arbitrary(u: &mut Unstructured) -> Result<Self> {
        log::trace!("New GearCalls generation: random data received {}", u.len());
        let test_input_id = get_sha1_string(u.peek_bytes(u.len()).expect("checked"));
        log::trace!("Generating GearCalls from corpus - {}", test_input_id);

        let config = ExtrinsicsConfig {
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
                    25,
                    Box::new(SendMessageGenerator {
                        gas: default_gas_limit(),
                        value: 0,
                        prepaid: false,
                    }),
                ),
            ],
        };

        if u.len() < config.min_bytes_for_generation() {
            log::trace!("Not enough bytes for creating gear calls");
            return Err(arbitrary::Error::NotEnoughData);
        }

        Ok(GearCalls(config.generate_calls(u)?))
    }
}

type ExtrinsicAmount = usize;

struct ExtrinsicsConfig {
    generators: Vec<(ExtrinsicAmount, Box<dyn ExtrinsicGenerator>)>,
}

impl ExtrinsicsConfig {
    fn generate_calls(
        self,
        unstructured: &mut Unstructured,
    ) -> Result<[RawGearCall; GearCalls::MAX_CALLS]> {
        let mut calls = get_uninitialized_calls();

        if calls.len() < self.overall_extrinsic_amount() {
            log::trace!("Too much gear calls are configured to generate");
            return Err(arbitrary::Error::IncorrectFormat);
        }

        let mut last = 0;
        let mut intermediate_data = IntermediateData::default();
        for (amount, mut generator) in self.generators {
            for _ in 0..amount {
                calls[last].write(generator.generate(&mut intermediate_data, unstructured)?);
                last += 1;
            }
        }
        Ok(transmute_calls_to_init(calls))
    }

    fn min_bytes_for_generation(&self) -> usize {
        self.generators
            .iter()
            .map(|(amount, generator)| amount * generator.min_bytes_for_generation())
            .sum()
    }

    fn overall_extrinsic_amount(&self) -> usize {
        self.generators.iter().map(|(amount, _)| amount).sum()
    }
}

trait ExtrinsicGenerator {
    fn generate(
        &mut self,
        intermediate_data: &mut IntermediateData,
        unstructured: &mut Unstructured,
    ) -> Result<RawGearCall>;

    fn min_bytes_for_generation(&self) -> usize;
}

/// Data that is persistent between different [`ExtrinsicGenerator`]s calls.
#[derive(Default)]
struct IntermediateData {
    uploaded_programs: Vec<ProgramId>,
}

/// Data that's generated when fuzzer executes [`GearCall`]s. It's used to
/// finalize initialization of some extrinsics.
#[derive(Default)]
pub struct FuzzerRuntimeData {
    pub mailbox_messages: Vec<MessageId>,
}

impl RawGearCall {
    fn select_message_from_mailbox(
        message_id: MessageId,
        runtime_data: &FuzzerRuntimeData,
    ) -> MessageId {
        let mailbox_message_count = runtime_data.mailbox_messages.len();
        if mailbox_message_count == 0 {
            log::warn!("Cannot find mailbox messages. Using random MessageId");
            message_id
        } else {
            let id = u64::from_le_bytes(message_id.as_ref()[..8].try_into().unwrap());
            runtime_data.mailbox_messages[id as usize % mailbox_message_count]
        }
    }

    pub fn preprocess(self, runtime_data: &FuzzerRuntimeData) -> GearCall {
        match self.0 {
            GearCall::SendReply(SendReplyArgs(args)) => {
                let message_id = Self::select_message_from_mailbox(args.0, runtime_data);
                SendReplyArgs((message_id, args.1, args.2, args.3, args.4)).into()
            }
            GearCall::ClaimValue(ClaimValueArgs(message_id)) => {
                let message_id = Self::select_message_from_mailbox(message_id, runtime_data);
                ClaimValueArgs(message_id).into()
            }
            _ => self.0,
        }
    }
}

struct UploadProgramGenerator {
    gas: u64,
    value: u128,
    test_input_id: String,
}

impl ExtrinsicGenerator for UploadProgramGenerator {
    fn generate(
        &mut self,
        intermediate_data: &mut IntermediateData,
        unstructured: &mut Unstructured,
    ) -> Result<RawGearCall> {
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

        Ok(RawGearCall(
            UploadProgramArgs((code, salt, payload, self.gas, self.value)).into(),
        ))
    }

    fn min_bytes_for_generation(&self) -> usize {
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
        &mut self,
        intermediate_data: &mut IntermediateData,
        unstructured: &mut Unstructured,
    ) -> Result<RawGearCall> {
        let program_id = unstructured
            .choose(&intermediate_data.uploaded_programs)
            .copied()?;
        let payload = arbitrary_payload(unstructured)?;
        log::trace!(
            "Random data after payload (send_message) gen {}",
            unstructured.len()
        );
        log::trace!("Payload (send_message) length {:?}", payload.len());

        Ok(RawGearCall(
            SendMessageArgs((program_id, payload, self.gas, self.value, self.prepaid)).into(),
        ))
    }

    fn min_bytes_for_generation(&self) -> usize {
        // 512 KiB for payload.
        520 * 1024
    }
}

struct SendReplyGenerator {
    gas: u64,
    value: u128,
    prepaid: bool,
}

impl ExtrinsicGenerator for SendReplyGenerator {
    fn generate(
        &mut self,
        _: &mut IntermediateData,
        unstructured: &mut Unstructured,
    ) -> Result<RawGearCall> {
        let message_id = arbitary_message_id(unstructured)?;

        let payload = arbitrary_payload(unstructured)?;
        log::trace!(
            "Random data after payload (send_reply) gen {}",
            unstructured.len()
        );
        log::trace!("Payload (send_reply) length {:?}", payload.len());

        Ok(RawGearCall(
            SendReplyArgs((message_id, payload, self.gas, self.value, self.prepaid)).into(),
        ))
    }

    fn min_bytes_for_generation(&self) -> usize {
        // 512 KiB for payload.
        520 * 1024
    }
}

struct ClaimValueGenerator {}

impl ExtrinsicGenerator for ClaimValueGenerator {
    fn generate(
        &mut self,
        _: &mut IntermediateData,
        unstructured: &mut Unstructured,
    ) -> Result<RawGearCall> {
        let message_id = arbitary_message_id(unstructured)?;
        Ok(RawGearCall(ClaimValueArgs(message_id).into()))
    }

    fn min_bytes_for_generation(&self) -> usize {
        // 32 bytes for message id.
        100
    }
}

fn arbitary_message_id(u: &mut Unstructured) -> Result<MessageId> {
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

fn get_uninitialized_calls() -> [MaybeUninit<RawGearCall>; GearCalls::MAX_CALLS] {
    unsafe {
        // # Safety:
        //
        // Create an uninitialized array of `MaybeUninit`. The `assume_init` is
        // safe because the type we are claiming to have initialized here is a
        // bunch of `MaybeUninit`s, which do not require initialization.
        MaybeUninit::uninit().assume_init()
    }
}

fn transmute_calls_to_init(
    uninit_calls: [MaybeUninit<RawGearCall>; GearCalls::MAX_CALLS],
) -> [RawGearCall; GearCalls::MAX_CALLS] {
    unsafe {
        // # Safety:
        //
        // Called when gear calls are initialized. Transmute the array to the
        // initialized type.
        mem::transmute::<_, [RawGearCall; GearCalls::MAX_CALLS]>(uninit_calls)
    }
}

fn config(
    programs: &[ProgramId],
    log_info: Option<String>,
) -> StandardGearWasmConfigsBundle<ProgramId> {
    let mut injection_amounts = SysCallsInjectionAmounts::all_once();
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
