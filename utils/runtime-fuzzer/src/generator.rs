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

mod send_message;
mod upload_program;

use gear_call_gen::GearCall;
use gear_common::event::ProgramChangeKind;
use gear_core::ids::ProgramId;
use gear_wasm_gen::wasm_gen_arbitrary::{Result, Unstructured};
use pallet_balances::Pallet as BalancesPallet;
use pallet_gear::Event as GearEvent;
use runtime_primitives::AccountId;
use std::{
    collections::HashSet,
    mem,
};
use vara_runtime::{Runtime, RuntimeEvent, System};

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

pub(crate) struct RuntimeInterimState {
    programs: HashSet<ProgramId>,
}

impl RuntimeInterimState {
    pub(crate) fn build() -> Self {
        let mut programs = HashSet::new();
        System::events().iter().for_each(|e| {
            if let RuntimeEvent::Gear(GearEvent::ProgramChanged {
                change: ProgramChangeKind::Active { .. },
                id,
                ..
            }) = e.event
            {
                programs.insert(id);
            }
        });

        Self { programs }
    }

    fn merge(&mut self, Self { programs }: Self) {
        self.programs.extend(programs);
    }
}

pub(crate) struct GenerationEnvironment<'a> {
    corpus_id: &'a str,
    existing_programs: HashSet<ProgramId>,
    max_gas: u64,
}

pub(crate) struct GenerationEnvironmentProducer<'a> {
    corpus_id: String,
    _unstructured: Unstructured<'a>,
    sender: AccountId,
    interim_state: Option<RuntimeInterimState>,
}

impl<'a> GenerationEnvironmentProducer<'a> {
    pub(crate) fn new(
        corpus_id: String,
        data_requirement: FulfilledDataRequirement<'a, Self>,
    ) -> Self {
        Self {
            corpus_id,
            _unstructured: Unstructured::new(data_requirement.data),
            sender: runtime::alice(),
            interim_state: None,
        }
    }

    pub(crate) fn produce_generation_env(
        &mut self,
        new_interim_state: RuntimeInterimState,
    ) -> GenerationEnvironment {
        if let Some(current_interim_state) = self.interim_state.as_mut() {
            current_interim_state.merge(new_interim_state);
        } else {
            self.interim_state = Some(new_interim_state);
        }

        runtime::increase_to_max_balance(self.sender.clone())
            .unwrap_or_else(|e| unreachable!("Balance update failed: {e:?}"));
        log::info!(
            "Current balance of the sender - {}",
            BalancesPallet::<Runtime>::free_balance(&self.sender)
        );

        let existing_programs = self
            .interim_state
            .as_ref()
            .map(|state| state.programs.clone())
            .expect("interim state is always `Some`; qed");

        GenerationEnvironment {
            corpus_id: &self.corpus_id,
            existing_programs,
            max_gas: runtime::default_gas_limit(),
        }
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
            .then(|| {
                self.generated_upload_program += 1;
                Self::UPLOAD_PROGRAM_CALL_ID
            })
            .or(
                (self.generated_send_message < Self::MAX_SEND_MESSAGE_CALLS)
                    .then(|| {
                        self.generated_send_message += 1;
                        Self::SEND_MESSAGE_CALL_ID
                    }),
            )
        else {
            return Ok(None);
        };

        match call_id {
            Self::UPLOAD_PROGRAM_CALL_ID => upload_program::generate(&mut self.unstructured, env),
            Self::SEND_MESSAGE_CALL_ID => send_message::generate(&mut self.unstructured, env),
            _ => unimplemented!("Unknown call id"),
        }
        .map(Some)
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

impl GenerationEnvironmentProducer<'_> {
    pub(crate) const fn random_data_requirement() -> usize {
        const VALUE_SIZE: usize = mem::size_of::<u128>();

        VALUE_SIZE
            * (GearCallsGenerator::MAX_UPLOAD_PROGRAM_CALLS
                + GearCallsGenerator::MAX_SEND_MESSAGE_CALLS)
            + AUXILIARY_SIZE
    }
}

fn arbitrary_payload(u: &mut Unstructured) -> Result<Vec<u8>> {
    arbitrary_limited_bytes(u, MAX_PAYLOAD_SIZE)
}

fn arbitrary_limited_bytes(u: &mut Unstructured, limit: usize) -> Result<Vec<u8>> {
    let arb_size = u.int_in_range(0..=limit)?;
    u.bytes(arb_size).map(|bytes| bytes.to_vec())
}
