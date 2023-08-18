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

//! `Arbitrary` trait implementation for a collection of [`GearCall`].

use crate::{runtime::default_gas_limit, GearCall, SendMessageArgs, UploadProgramArgs};
use arbitrary::{Arbitrary, Result, Unstructured};
use gear_core::ids::{CodeId, ProgramId};
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

/// New-type wrapper over array of [`GearCall`]s.
///
/// It's main purpose is to be an implementor of `Arbitrary` for the array of [`GearCall`]s.
/// New-type is required as array is always a foreign type.
#[derive(Clone)]
pub struct GearCalls(pub [GearCall; GearCalls::MAX_CALLS]);

/// That's done because when fuzzer finds a crash it prints a [`Debug`] string of the [`GearCalls`].
/// Fuzzer executes [`GearCalls`] with pretty large codes and payloads, therefore to avoid printing huge
/// amount of data we do a mock implementation of [`Debug`].
///
/// If one wants to see a real debug string of the type, a separate wrapper over [`GearCall`]s array must be
/// implemented. This wrapper must implement [`Debug`] then.
impl Debug for GearCalls {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_tuple("GearCalls")
            .field(&"Mock `Debug` impl")
            .finish()
    }
}

impl GearCalls {
    pub const MAX_CALLS: usize = GearCalls::INIT_MSGS + GearCalls::HANDLE_MSGS;
    pub const INIT_MSGS: usize = 10;
    pub const HANDLE_MSGS: usize = 25;
}

impl<'a> Arbitrary<'a> for GearCalls {
    fn arbitrary(u: &mut Unstructured<'a>) -> Result<Self> {
        // Newline to easily browse logs.
        println!("\n");

        log::trace!("New GearCalls generation: random data received {}", u.len());

        // 25 MiB is an approximate assessment for 35 calls,
        // where each call of:
        // 1. `UploadProgram` requires 1024 KiB for payload an salt and 50 KiB for code.
        // 2. `SendMessage` requires 512 Kib for payload.
        // So, [10 * (1024 + 50)] + [25 * 512] = 23540 KiB
        if u.len() < 25_000_000_usize {
            log::trace!("Not enough bytes for creating gear calls");
            return Err(arbitrary::Error::NotEnoughData);
        }

        let log_data = format!(
            "Generated from corpus - {}",
            get_sha1_string(u.peek_bytes(u.len()).expect("checked"))
        );
        let gas = default_gas_limit();
        let value = 0;
        let prepaid = false;
        let mut programs = [ProgramId::default(); GearCalls::INIT_MSGS];
        // Upload code used as a default value.
        let mut calls = get_uninitialized_calls();

        // Generate `GearCalls::INIT_MSGS` number of `UploadProgram` calls.
        for i in 0..GearCalls::INIT_MSGS {
            log::trace!("New gear-wasm generation");

            log::trace!("Random data before wasm gen {}, iter - {i}", u.len());

            let code = gear_wasm_gen::generate_gear_program_code(
                u,
                config(programs, Some(log_data.clone())),
            )?;
            log::trace!("Random data after wasm gen {}, iter - {i}", u.len());
            log::trace!("Code length {:?}", code.len());

            let salt = arbitrary_salt(u)?;
            log::trace!("Random data after salt gen {}, iter - {i}", u.len());
            log::trace!("Salt length {:?}", salt.len());

            let payload = arbitrary_payload(u)?;
            log::trace!(
                "Random data after payload (upload_program) gen {}, iter - {i}",
                u.len()
            );
            log::trace!("Payload (upload_program) length {:?}", payload.len());

            let program_id = ProgramId::generate(CodeId::generate(&code), &salt);

            log::trace!("Generated code for program id - {program_id}");

            programs[i] = program_id;
            calls[i].write(GearCall::UploadProgram(UploadProgramArgs((
                code, salt, payload, gas, value,
            ))));
        }

        // Generate `GearCalls::HANDLE_MSGS` number of `SendMessage` calls.
        #[allow(clippy::needless_range_loop)]
        for i in GearCalls::INIT_MSGS..GearCalls::INIT_MSGS + GearCalls::HANDLE_MSGS {
            let program_id = u.choose(&programs).copied()?;
            let payload = arbitrary_payload(u)?;
            log::trace!(
                "Random data after payload (send_message) gen {}, iter - {i}",
                u.len()
            );
            log::trace!("Payload (send_message) length {:?}", payload.len());

            calls[i].write(GearCall::SendMessage(SendMessageArgs((
                program_id, payload, gas, value, prepaid,
            ))));
        }

        let calls = transmute_calls_to_init(calls);

        log::trace!(
            "GearCalls generation ended. Random data remains - {}",
            u.len()
        );

        Ok(GearCalls(calls))
    }
}

fn arbitrary_salt(u: &mut Unstructured<'_>) -> Result<Vec<u8>> {
    arbitrary_limited_bytes(u, MAX_PAYLOAD_SIZE)
}

fn arbitrary_payload(u: &mut Unstructured<'_>) -> Result<Vec<u8>> {
    arbitrary_limited_bytes(u, MAX_PAYLOAD_SIZE)
}

fn arbitrary_limited_bytes(u: &mut Unstructured<'_>, limit: usize) -> Result<Vec<u8>> {
    let arb_size = u.int_in_range(0..=limit)?;
    u.bytes(arb_size).map(|bytes| bytes.to_vec())
}

fn get_uninitialized_calls() -> [MaybeUninit<GearCall>; GearCalls::MAX_CALLS] {
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
    uninit_calls: [MaybeUninit<GearCall>; GearCalls::MAX_CALLS],
) -> [GearCall; GearCalls::MAX_CALLS] {
    unsafe {
        // # Safety:
        //
        // Called when gear calls are initialized. Transmute the array to the
        // initialized type.
        mem::transmute::<_, [GearCall; GearCalls::MAX_CALLS]>(uninit_calls)
    }
}

fn config(
    programs: [ProgramId; GearCalls::INIT_MSGS],
    log_info: Option<String>,
) -> StandardGearWasmConfigsBundle<ProgramId> {
    let mut injection_amounts = SysCallsInjectionAmounts::all_once();
    injection_amounts.set(SysCallName::Leave, 0, 0);
    injection_amounts.set(SysCallName::Panic, 0, 0);
    injection_amounts.set(SysCallName::OomPanic, 0, 0);
    injection_amounts.set(SysCallName::Send, 20, 30);
    injection_amounts.set(SysCallName::Exit, 0, 1);

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
        ..Default::default()
    }
}

fn get_sha1_string(input: &[u8]) -> String {
    let mut hasher = sha1::Sha1::new();
    hasher.update(input);

    hex::encode(hasher.finalize())
}
