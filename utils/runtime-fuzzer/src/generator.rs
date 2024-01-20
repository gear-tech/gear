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

use arbitrary::{Result, Unstructured};
use gear_call_gen::{GearCall, SendMessageArgs, UploadProgramArgs};
use gear_core::ids::{CodeId, ProgramId};
use gear_utils::NonEmpty;
use gear_wasm_gen::{
    ActorKind, EntryPointsSet, InvocableSyscall, PtrParamAllowedValues, RegularParamType,
    StandardGearWasmConfigsBundle, SyscallName, SyscallsInjectionTypes, SyscallsParamsConfig,
};
use pallet_balances::Pallet as BalancesPallet;
use runtime_primitives::AccountId;
use std::mem;
use vara_runtime::Runtime;

use crate::{data::*, runtime};

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
const GAS_AND_VALUE_SIZE: usize = mem::size_of::<(u64, u128)>();

/// Used to make sure that generators will not exceed `Unstructured` size as it's used not only
/// to generate things like wasm code or message payload but also to generate some auxiliary
/// data, for example index in some vec.
const AUXILIARY_SIZE: usize = 512;

pub(crate) struct RuntimeInterimState;
pub(crate) struct GenerationEnvironment {
    corpus_id: String,
    existing_programs: Vec<ProgramId>,
    max_gas: u64,
}

pub(crate) struct GenerationEnvironmentProducer<'a> {
    unstructured: Unstructured<'a>,
    sender: AccountId,
    interim_state: Option<RuntimeInterimState>,
}

impl<'a> GenerationEnvironmentProducer<'a> {
    pub(crate) fn new(data_requirement: FulfilledDataRequirement<'a, Self>) -> Self {
        Self {
            unstructured: Unstructured::new(data_requirement.data),
            sender: runtime::alice(),
            interim_state: None,
        }
    }

    fn produce_generation_env(
        &mut self,
        prev_exec_interim_state: Option<RuntimeInterimState>,
    ) -> GenerationEnvironment {
        {
            todo!("merge prev_exec_interim_state and interim_state")
        }
        runtime::increase_to_max_balance(self.sender.clone())
            .unwrap_or_else(|e| unreachable!("Balance update failed: {e:?}"));
        log::info!(
            "Current balance of the sender - {}",
            BalancesPallet::<Runtime>::free_balance(&self.sender)
        );

        GenerationEnvironment {
            corpus_id: todo!(),
            existing_programs: todo!(),
            max_gas: todo!(),
        }
    }
}

impl GenerationEnvironmentProducer<'_> {
    pub(crate) const fn random_data_requirement() -> usize {
        const VALUE_SIZE: usize = mem::size_of::<u128>();

        VALUE_SIZE
            * (GearCallsGenerator::MAX_UPLOAD_PROGRAM_CALLS
                + GearCallsGenerator::MAX_SEND_MESSAGE_CALLS)
            + AUXILIARY_SIZE
    }
}

pub(crate) struct GearCallsGenerator<'a> {
    unstructured: Unstructured<'a>,
    generated_upload_program: usize,
    generated_send_message: usize,
    // generated_send_reply: usize,
}

impl<'a> GearCallsGenerator<'a> {
    const UPLOAD_PROGRAM_CALL_ID: usize = 0;
    const SEND_MESSAGE_CALL_ID: usize = 1;

    pub(crate) fn new(data_requirement: FulfilledDataRequirement<'a, Self>) -> Self {
        Self {
            unstructured: Unstructured::new(data_requirement.data),
            generated_upload_program: 0,
            generated_send_message: 0,
        }
    }

    pub(crate) fn generate(&mut self, env: GenerationEnvironment) -> Result<Option<GearCall>> {
        let Some(call_id) = (self.generated_upload_program < Self::MAX_UPLOAD_PROGRAM_CALLS)
            .then_some(Self::UPLOAD_PROGRAM_CALL_ID)
            .or_else(|| (self.generated_upload_program < Self::MAX_SEND_MESSAGE_CALLS).then_some(Self::SEND_MESSAGE_CALL_ID)) else {
                return Ok(None)
            };

        match call_id {
            Self::UPLOAD_PROGRAM_CALL_ID => {
                generate_upload_program(&mut self.unstructured, env).map(Some)
            }
            Self::SEND_MESSAGE_CALL_ID => todo!(),
            _ => unimplemented!("Unknown call id"),
        }
    }
}

fn generate_upload_program(
    unstructured: &mut Unstructured,
    env: GenerationEnvironment,
) -> Result<GearCall> {
    log::trace!("New gear-wasm generation");
    log::trace!("Random data before wasm gen {}", unstructured.len());

    let GenerationEnvironment {
        corpus_id,
        existing_programs,
        max_gas,
    } = env;

    let code = gear_wasm_gen::generate_gear_program_code(
        unstructured,
        config(
            &existing_programs,
            Some(format!("Generated program from corpus - {corpus_id}")),
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

    Ok(UploadProgramArgs((code, salt, payload, max_gas, 0)).into())
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

fn config(programs: &[ProgramId], log_info: Option<String>) -> StandardGearWasmConfigsBundle {
    let initial_pages = 2;
    let mut injection_types = SyscallsInjectionTypes::all_once();
    injection_types.set_multiple(
        [
            (SyscallName::Leave, 0..=0),
            (SyscallName::Panic, 0..=0),
            (SyscallName::OomPanic, 0..=0),
            (SyscallName::EnvVars, 0..=0),
            (SyscallName::Send, 10..=15),
            (SyscallName::Exit, 0..=1),
            (SyscallName::Alloc, 3..=6),
            (SyscallName::Free, 3..=6),
        ]
        .map(|(syscall, range)| (InvocableSyscall::Loose(syscall), range))
        .into_iter(),
    );

    let mut params_config = SyscallsParamsConfig::new()
        .with_default_regular_config()
        .with_rule(RegularParamType::Alloc, (10..=20).into())
        .with_rule(
            RegularParamType::Free,
            (initial_pages..=initial_pages + 35).into(),
        )
        .with_ptr_rule(PtrParamAllowedValues::Value(0..=0));

    let actor_kind = NonEmpty::collect(
        programs
            .iter()
            .copied()
            .filter_map(|pid| (pid != ProgramId::default()).then_some(pid.into())),
    )
    .map(ActorKind::ExistingAddresses)
    .unwrap_or(ActorKind::Source);

    log::trace!("Messages destination config: {:?}", actor_kind);

    params_config = params_config
        .with_ptr_rule(PtrParamAllowedValues::ActorId(actor_kind.clone()))
        .with_ptr_rule(PtrParamAllowedValues::ActorIdWithValue {
            actor_kind: actor_kind.clone(),
            range: 0..=0,
        });

    StandardGearWasmConfigsBundle {
        entry_points_set: EntryPointsSet::InitHandleHandleReply,
        injection_types,
        log_info,
        params_config,
        initial_pages: initial_pages as u32,
        ..Default::default()
    }
}

impl GearCallsGenerator<'_> {
    // *WARNING*:
    //
    // Increasing these constants requires resetting minimal
    // size of fuzzer input buffer in corresponding scripts.
    const MAX_UPLOAD_PROGRAM_CALLS: usize = 10;
    const MAX_SEND_MESSAGE_CALLS: usize = 15;

    pub(crate) const fn random_data_requirement() -> usize {
        Self::upload_program_data_requirement() * Self::MAX_UPLOAD_PROGRAM_CALLS
            + Self::send_message_data_requirement() * Self::MAX_SEND_MESSAGE_CALLS
    }

    const fn upload_program_data_requirement() -> usize {
        MAX_CODE_SIZE + MAX_SALT_SIZE + MAX_PAYLOAD_SIZE + GAS_AND_VALUE_SIZE + AUXILIARY_SIZE
    }

    const fn send_message_data_requirement() -> usize {
        ID_SIZE + MAX_PAYLOAD_SIZE + GAS_AND_VALUE_SIZE + AUXILIARY_SIZE
    }
}
