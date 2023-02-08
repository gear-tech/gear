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

<<<<<<< HEAD
//! Generator of the `pallet-gear` calls.
=======
//! Generator of the `pallet-gear` calls
>>>>>>> 549b51aa8 (Initial)

mod create_program;
mod rand_utils;
mod send_message;
mod upload_code;
mod upload_program;

pub type Seed = u64;

pub use create_program::CreateProgramArgs;
<<<<<<< HEAD
pub use rand_utils::{CallGenRng, CallGenRngCore};
=======
pub use rand_utils::CallGenRng;
>>>>>>> 549b51aa8 (Initial)
pub use send_message::SendMessageArgs;
pub use upload_code::UploadCodeArgs;
pub use upload_program::UploadProgramArgs;

/// Set of `pallet_gear` calls supported by the crate.
<<<<<<< HEAD
=======
// todo [sab] possibly not needed?
>>>>>>> 549b51aa8 (Initial)
pub enum GearCall {
    /// Upload program call args.
    UploadProgram(UploadProgramArgs),
    /// Send message call args.
    SendMessage(SendMessageArgs),
    /// Create program call args.
    CreateProgram(CreateProgramArgs),
    /// Upload program call args.
    UploadCode(UploadCodeArgs),
}

pub fn generate_gear_program<Rng: CallGenRng>(seed: u64) -> Vec<u8> {
    use arbitrary::Unstructured;

    let mut rng = Rng::seed_from_u64(seed);

    let mut buf = vec![0; 100_000];
    rng.fill_bytes(&mut buf);

    let mut u = Unstructured::new(&buf);

    let mut config = gear_wasm_gen::GearConfig::new_normal();
    config.print_test_info = Some(format!("Gear program seed = '{seed}'"));

    gear_wasm_gen::gen_gear_program_code(&mut u, config)
}
