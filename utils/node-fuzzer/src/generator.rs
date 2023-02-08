// // This file is part of Gear.

// // Copyright (C) 2021-2023 Gear Technologies Inc.
// // SPDX-License-Identifier: GPL-3.0-or-later WITH Classpath-exception-2.0

// // This program is free software: you can redistribute it and/or modify
// // it under the terms of the GNU General Public License as published by
// // the Free Software Foundation, either version 3 of the License, or
// // (at your option) any later version.

// // This program is distributed in the hope that it will be useful,
// // but WITHOUT ANY WARRANTY; without even the implied warranty of
// // MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE. See the
// // GNU General Public License for more details.

// // You should have received a copy of the GNU General Public License
// // along with this program. If not, see <https://www.gnu.org/licenses/>.

// use arbitrary::Unstructured;
// use gear_core::{{utils::NonEmpty, RingGet}, ids::{ProgramId, CodeId}};
// use rand::{rngs::SmallRng, Rng, RngCore, SeedableRng};

// trait Context {
//     fn get_programs(&self) -> Vec<ProgramId>;
//     fn get_code(&self) -> Vec<CodeId>;
// }

// // code, salt, payload, gas, value
// type UploadProgramArgsInner = (Vec<u8>, Vec<u8>, Vec<u8>, u64, u128);
// // destination, payload, gas, value
// type SendMessageArgsInner = (ProgramId, Vec<u8>, u64, u128);

// pub enum GearCalls {
//     UploadProgram(UploadProgramArgs),
//     SendMessage(SendMessageArgs),
// }

// pub struct UploadProgramArgs(UploadProgramArgsInner);

// pub struct SendMessageArgs(SendMessageArgsInner);

// impl UploadProgramArgs {
//     pub fn generate(code_seed: u64, rng_seed: u64, gas_limit: u64) -> Self {
//         let mut rng = SmallRng::seed_from_u64(rng_seed);

//         let code = generate_gear_program(code_seed);

//         let mut salt = vec![0; rng.gen_range(1..=100)];
//         rng.fill_bytes(&mut salt);

//         let mut payload = vec![0; rng.gen_range(1..=100)];
//         rng.fill_bytes(&mut payload);

//         log::debug!(
//             "Generated `upload_program` batch with code seed = {code_seed}, salt = {}, payload = {}",
//             hex::encode(&salt),
//             hex::encode(&payload)
//         );

//         // todo generate value randomly too
//         let value = 0;

//         Self((code, salt, payload, gas_limit, value))
//     }
// }

// fn generate_gear_program(seed: u64) -> Vec<u8> {
//     let mut rng = SmallRng::seed_from_u64(seed);

//     let mut buf = vec![0; 100_000];
//     rng.fill_bytes(&mut buf);

//     let mut u = Unstructured::new(&buf);

//     let mut config = gear_wasm_gen::GearConfig::new_normal();
//     config.print_test_info = Some(format!("Gear program seed = '{seed}'"));

//     gear_wasm_gen::gen_gear_program_code(&mut u, config)
// }

// impl SendMessageArgs {
//     pub fn generate(existing_programs: NonEmpty<ProgramId>, rng_seed: u64, gas_limit: u64) -> Self {
//         let mut rng = SmallRng::seed_from_u64(rng_seed);

//         let program_idx = rng.next_u64() as usize;
//         let &destination = existing_programs
//             .ring_get(program_idx);

//         let mut payload = vec![0; rng.gen_range(1..=100)];
//         rng.fill_bytes(&mut payload);

//         log::debug!(
//             "Generated `send_message` batch with destination = {destination}, payload = {}",
//             hex::encode(&payload)
//         );

//         let value = 0;

//         Self((destination, payload, gas_limit, value))
//     }
// }
