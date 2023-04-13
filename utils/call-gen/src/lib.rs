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

//! Generator of the `pallet-gear` calls.

mod claim_value;
mod create_program;
mod rand_utils;
mod send_message;
mod send_reply;
mod upload_code;
mod upload_program;

pub use claim_value::ClaimValueArgs;
pub use create_program::CreateProgramArgs;
use gear_core::ids::ProgramId;
pub use rand_utils::{CallGenRng, CallGenRngCore};
pub use send_message::SendMessageArgs;
pub use send_reply::SendReplyArgs;
pub use upload_code::UploadCodeArgs;
pub use upload_program::UploadProgramArgs;

#[derive(Debug, Clone, thiserror::Error)]
#[error("Can't convert to gear call {0:?} call")]
pub struct GearCallConversionError(pub &'static str);

pub type Seed = u64;
pub type GearProgGenConfig = gear_wasm_gen::GearConfig;

/// Set of `pallet_gear` calls supported by the crate.
pub enum GearCall {
    /// Upload program call args.
    UploadProgram(UploadProgramArgs),
    /// Send message call args.
    SendMessage(SendMessageArgs),
    /// Create program call args.
    CreateProgram(CreateProgramArgs),
    /// Upload program call args.
    UploadCode(UploadCodeArgs),
    /// Send reply call args.
    SendReply(SendReplyArgs),
    /// Claim value call args.
    ClaimValue(ClaimValueArgs),
}

/// Function generates WASM-binary of a Gear program with the
/// specified `seed`. `programs` may specify addresses which
/// can be used for send-calls.
pub fn generate_gear_program<Rng: CallGenRng>(
    seed: u64,
    mut config: GearProgGenConfig,
    programs: Vec<ProgramId>,
) -> Vec<u8> {
    use arbitrary::Unstructured;
    use gear_wasm_gen::gsys;

    let mut rng = Rng::seed_from_u64(seed);

    let mut buf = vec![0; 100_000];
    rng.fill_bytes(&mut buf);

    let mut u = Unstructured::new(&buf);

    config.print_test_info = Some(format!("Gear program seed = '{seed}'"));

    let addresses = programs
        .iter()
        .map(|pid| gsys::HashWithValue {
            hash: pid.into_bytes(),
            value: 0,
        })
        .collect::<Vec<_>>();

    gear_wasm_gen::gen_gear_program_code(&mut u, config, &addresses)
}
