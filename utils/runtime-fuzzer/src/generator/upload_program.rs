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

use super::{
    RuntimeStateView, AUXILIARY_SIZE, GAS_SIZE, MAX_CODE_SIZE, MAX_PAYLOAD_SIZE, MAX_SALT_SIZE,
    VALUE_SIZE,
};
use gear_call_gen::{GearCall, UploadProgramArgs};
use gear_core::ids::{CodeId, ProgramId};
use gear_utils::NonEmpty;
use gear_wasm_gen::{
    wasm_gen_arbitrary::{Result, Unstructured},
    ActorKind, EntryPointsSet, InvocableSyscall, PtrParamAllowedValues, RegularParamType,
    StandardGearWasmConfigsBundle, SyscallName, SyscallsInjectionTypes, SyscallsParamsConfig,
};
use runtime_primitives::Balance;

pub(crate) type UploadProgramRuntimeData<'a> = (&'a str, Option<&'a NonEmpty<ProgramId>>, u64);

pub(super) const fn data_requirement() -> usize {
    MAX_CODE_SIZE + MAX_SALT_SIZE + MAX_PAYLOAD_SIZE + GAS_SIZE + VALUE_SIZE + AUXILIARY_SIZE
}

impl<'a> From<RuntimeStateView<'a>> for UploadProgramRuntimeData<'a> {
    fn from(env: RuntimeStateView<'a>) -> Self {
        (env.corpus_id, env.programs, env.max_gas)
    }
}

impl<'a> From<RuntimeStateView<'a>> for UploadProgramRuntimeData<'a> {
    fn from(env: RuntimeStateView<'a>) -> Self {
        (
            env.corpus_id,
            env.programs,
            env.max_gas,
            env.current_balance,
        )
    }
}

pub(crate) fn generate(
    unstructured: &mut Unstructured,
    (corpus_id, programs, gas, current_balance): UploadProgramRuntimeData,
) -> Result<GearCall> {
    log::trace!("New gear-wasm generation");
    log::trace!("Random data before wasm gen {}", unstructured.len());

    let code = gear_wasm_gen::generate_gear_program_code(
        unstructured,
        config(
            programs,
            Some(format!("Generated program from corpus - {corpus_id}")),
            current_balance,
        ),
    )?;
    log::trace!("Random data after wasm gen {}", unstructured.len());
    log::trace!("Code length {:?}", code.len());

    let salt = arbitrary_salt(unstructured)?;
    log::trace!("Random data after salt gen {}", unstructured.len());
    log::trace!("Salt length {:?}", salt.len());

    let payload = super::arbitrary_payload(unstructured)?;
    log::trace!(
        "Random data after payload (upload_program) gen {}",
        unstructured.len()
    );
    log::trace!("Payload (upload_program) length {:?}", payload.len());

    let value = unstructured.int_in_range(0..=current_balance.saturating_div(7))?;
    log::trace!("Random data after value generation {}", unstructured.len());
    log::trace!("Sending value (upload_program) - {value}");

    let program_id = ProgramId::generate_from_user(CodeId::generate(&code), &salt);
    log::trace!("Generated code for program id - {program_id}");

    Ok(UploadProgramArgs((code, salt, payload, gas, value)).into())
}

fn arbitrary_salt(u: &mut Unstructured) -> Result<Vec<u8>> {
    super::arbitrary_limited_bytes(u, MAX_SALT_SIZE)
}

fn config(
    programs: Option<&NonEmpty<ProgramId>>,
    log_info: Option<String>,
    current_balance: Balance,
) -> StandardGearWasmConfigsBundle {
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
        .with_ptr_rule(PtrParamAllowedValues::Value(
            0..=current_balance.saturating_div(1000),
        ));

    let actor_kind = programs
        .cloned()
        .and_then(|non_empty| NonEmpty::collect(non_empty.into_iter().map(|pid| pid.into())))
        .map(ActorKind::ExistingAddresses)
        .unwrap_or(ActorKind::Source);

    log::trace!("Messages destination config: {:?}", actor_kind);

    params_config = params_config
        .with_ptr_rule(PtrParamAllowedValues::ActorId(actor_kind.clone()))
        .with_ptr_rule(PtrParamAllowedValues::ActorIdWithValue {
            actor_kind: actor_kind.clone(),
            // TODO: reconsider that !!!
            range: 0..=current_balance.saturating_div(1000),
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
