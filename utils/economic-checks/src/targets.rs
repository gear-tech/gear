// This file is part of Gear.

// Copyright (C) 2021-2022 Gear Technologies Inc.
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

use crate::{util::*, Params, MAX_QUEUE_LEN};
use arbitrary::Unstructured;
use codec::Encode;
use common::GasTree;
use demo_contract_template::WASM_BINARY as GENERAL_WASM_BINARY;
use demo_mul_by_const::WASM_BINARY as MUL_CONST_WASM_BINARY;
use demo_ncompose::WASM_BINARY as NCOMPOSE_WASM_BINARY;
use frame_support::dispatch::DispatchError;
use gear_core::ids::ProgramId;
#[cfg(feature = "gear-native")]
use gear_runtime::{Gear, Runtime, RuntimeOrigin};
use pallet_gear::GasHandlerOf;
use primitive_types::H256;
use rand::{rngs::StdRng, seq::SliceRandom, RngCore, SeedableRng};
use sp_core::sr25519;
use sp_std::collections::btree_map::BTreeMap;
use std::fmt;
#[cfg(all(not(feature = "gear-native"), feature = "vara-native"))]
use vara_runtime::{Gear, Runtime, RuntimeOrigin};
use wasm_mutate::{ErrorKind, WasmMutate};
use wasmparser::Validator;

type TargetOutcome = Result<GasUsageStats, DispatchError>;

struct Seed([u8; 32]);

impl fmt::Display for Seed {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "0x{}", hex::encode(self.0))
    }
}

impl fmt::Debug for Seed {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        fmt::Display::fmt(self, f)
    }
}

const MAX_BLOCK: u16 = 20;
const MIN_BLOCK: u16 = 3;
const NUM_TRIES: u16 = 5;

#[derive(Default, Debug)]
pub struct GasUsageStats {
    total_gas_supply: u64,
    accounted_gas: u64,
    total_balance: u128,
    initial_balance: u128,
    #[allow(unused)]
    reserved_balance: u128,
}

impl GasUsageStats {
    fn new(
        total_gas: u64,
        accounted_gas: u64,
        total_balance: u128,
        initial_balance: u128,
        reserved_balance: u128,
    ) -> Self {
        Self {
            total_gas_supply: total_gas,
            accounted_gas,
            total_balance,
            initial_balance,
            reserved_balance,
        }
    }
}

pub fn composer_target(params: &Params) -> TargetOutcome {
    let alice = get_account_id_from_seed::<sr25519::Public>("Alice");
    let (mut ext, pool) = with_offchain_ext(
        vec![(alice.clone(), 1_000_000_000_000_000_u128)],
        vec!["Val"],
        alice.clone(),
    );
    ext.execute_with(|| {
        // Initial value in all gas trees is 0
        if GasHandlerOf::<Runtime>::total_supply() != 0 || total_gas_in_wl_mb() != 0 {
            return Ok(GasUsageStats::new(
                GasHandlerOf::<Runtime>::total_supply(),
                total_gas_in_wl_mb(),
                0,
                0,
                0,
            ));
        }

        if let Params::Composer(params) = params {
            let composer_id = generate_program_id(NCOMPOSE_WASM_BINARY, b"salt");
            let mul_id = generate_program_id(MUL_CONST_WASM_BINARY, b"salt");

            Gear::upload_program(
                RuntimeOrigin::signed(alice.clone()),
                MUL_CONST_WASM_BINARY.to_vec(),
                b"salt".to_vec(),
                params.intrinsic_value.encode(),
                2_500_000_000,
                0,
            )
            .map_err(|e| e.error)?;

            Gear::upload_program(
                RuntimeOrigin::signed(alice.clone()),
                NCOMPOSE_WASM_BINARY.to_vec(),
                b"salt".to_vec(),
                (<[u8; 32]>::from(mul_id), params.depth).encode(),
                2_500_000_000,
                0,
            )
            .map_err(|e| e.error)?;

            run_to_block_with_ocw(2, &pool, None);

            Gear::send_message(
                RuntimeOrigin::signed(alice.clone()),
                composer_id,
                1_u64.to_le_bytes().to_vec(),
                params.gas_limit,
                0,
            )
            .map_err(|e| e.error)?;

            // Modeling offchain workers being run every certain number of blocks
            run_to_block_with_ocw(50, &pool, None);
        }

        // Gas balance adds up: all gas is held by waiting messages only
        Ok(GasUsageStats::new(
            GasHandlerOf::<Runtime>::total_supply(),
            total_gas_in_wl_mb(),
            0,
            0,
            0,
        ))
    })
}

pub fn run_target<F>(params: &Params, f: F)
where
    F: FnOnce(&Params) -> TargetOutcome,
{
    init_logger();

    log::debug!("[run_target] params = {:?}", params);
    match f(params) {
        Ok(outcome) => {
            log::debug!("[run_target] test outcome = {:?}", &outcome);
            assert_eq!(outcome.total_gas_supply, outcome.accounted_gas);
            assert_eq!(outcome.total_balance, outcome.initial_balance);
        }
        Err(e) => {
            log::debug!("[run_target] ERROR IN TARGET FUNCTION: {:?}", e);
            match e {
                legit_error_1
                    if legit_error_1 == pallet_gear::Error::<Runtime>::GasLimitTooHigh.into() => {}
                legit_error_2
                    if legit_error_2 == pallet_gear::Error::<Runtime>::InactiveProgram.into() => {}
                legit_error_3
                    if legit_error_3
                        == pallet_gear::Error::<Runtime>::NotEnoughBalanceForReserve.into() => {}
                legit_error_4
                    if legit_error_4
                        == pallet_gear::Error::<Runtime>::FailedToConstructProgram.into() => {}
                _ => panic!("{:?}", e),
            }
        }
    }
}

pub fn simple_scenario(params: &Params) -> TargetOutcome {
    if let Params::Simple(params) = params {
        // Initialize random generator with a seed
        let mut rng: StdRng = SeedableRng::from_seed(params.input);

        // Create a distribution of user accounts, mint funds
        let alice = get_account_id_from_seed::<sr25519::Public>("Alice");
        let accounts = create_random_accounts(&mut rng, &alice);
        log::debug!("Created balances for {} accounts", accounts.len());

        // Creating test externalities (with offchain workers support)
        let (mut ext, pool) = with_offchain_ext(accounts.clone(), vec!["Val"], alice.clone());
        ext.execute_with(|| {
            // Currency balance of all accounts (total issuance)
            let initial_total_balance =
                <Runtime as pallet_gear::Config>::Currency::total_issuance();

            // Initial value in all gas trees is 0
            if GasHandlerOf::<Runtime>::total_supply() != 0 || total_gas_in_wl_mb() != 0 {
                return Ok(GasUsageStats::new(
                    GasHandlerOf::<Runtime>::total_supply(),
                    total_gas_in_wl_mb(),
                    initial_total_balance,
                    initial_total_balance,
                    total_reserved_balance(),
                ));
            }

            // Generate test contracts
            let num_contracts = params.num_contracts as usize;
            let mut contracts = BTreeMap::<ProgramId, (Vec<u8>, Vec<u8>)>::new();
            let mut program_ids = Vec::<ProgramId>::new();

            // Generating IDs for contracts-to-be

            // Registering the original contract
            let original_code = GENERAL_WASM_BINARY;
            let salt = &0_usize.to_le_bytes()[..];
            let original_contract_id = generate_program_id(original_code, salt);
            contracts.insert(
                original_contract_id,
                (original_code.to_vec(), salt.to_vec()),
            );
            program_ids.push(original_contract_id);

            // Attempting to mutate the original wasm code to get as many contracts as necessary
            let mut mutator = WasmMutate::default();
            let mut fuel = 1000_u64;
            mutator.seed(params.seed).preserve_semantics(true);

            let mut count = 1_usize;
            for _ in 0..NUM_TRIES {
                if count >= num_contracts {
                    break;
                }
                mutator.fuel(fuel);
                let it = match mutator.run(original_code) {
                    Ok(it) => it,
                    Err(e) => {
                        log::debug!("Error mutating wasm: {:?}", e);
                        match e.kind() {
                            ErrorKind::NoMutationsApplicable => continue,
                            ErrorKind::OutOfFuel => {
                                fuel *= 2;
                                continue;
                            }
                            _ => return Err(DispatchError::from("Failed to mutate wasm")),
                        }
                    }
                };
                for mutated in it.take(num_contracts - 1) {
                    let mutated = mutated.expect("Expect mutated wasm");
                    let mut validator = Validator::new();
                    validator
                        .validate_all(&mutated[..])
                        .map_err(|_| DispatchError::from("Mutated wasm is invalid"))?;
                    let salt = &count.to_le_bytes()[..];
                    let contract_id = generate_program_id(&mutated[..], salt);
                    contracts.insert(contract_id, (mutated, salt.to_vec()));
                    program_ids.push(contract_id);
                    count += 1;
                    if count >= num_contracts {
                        break;
                    }
                }
            }

            // In case we have fewer contracts than desired, fill the empty slots with the original
            for i in count..num_contracts {
                let salt = &i.to_le_bytes()[..];
                let contract_id = generate_program_id(original_code, salt);
                contracts.insert(contract_id, (original_code.to_vec(), salt.to_vec()));
                program_ids.push(contract_id);
            }

            // Deploy test contracts by sending out init messages on behalf of users
            let payload = program_ids.clone().encode();
            for (_k, (c, s)) in contracts {
                let author = &accounts
                    .choose(&mut rng)
                    .expect("Accounts vec must not be empty")
                    .0;

                // Decide whether the message should contain value
                let value = match rng.next_u32() >> 30 {
                    0_u32 => rng.next_u32() as u128, // 25% chances to have value
                    _ => 0_u128,
                };

                Gear::upload_program(
                    RuntimeOrigin::signed(author.clone()),
                    c,
                    s,
                    payload.clone(),
                    2_500_000_000,
                    value,
                )
                .map_err(|e| e.error)?;
            }

            run_to_block_with_ocw(2, &pool, None);

            // Shuffle ID's of deployed programs with some random hashes in 4:1 ratio
            let num_random = num_contracts >> 2;
            let mut hashes = program_ids
                .into_iter()
                .chain(
                    (0..num_random)
                        .into_iter()
                        .map(|_| ProgramId::from(&H256::random()[..])),
                )
                .collect::<Vec<_>>();
            hashes.sort();
            let num_hashes: u16 = hashes.len() as u16;

            let mut bytes = [0u8; 2 * 36 * MAX_QUEUE_LEN as usize];
            rng.fill_bytes(&mut bytes);

            let u = Unstructured::new(&bytes[..]);

            // Generate messages queue
            let queue_len = params.queue_len as usize;
            let mut queue = BTreeMap::<u16, Vec<(u16, Seed)>>::new();

            u.arbitrary_take_rest_iter::<(u16, u16, [u8; 32])>()
                .map_err(|_| {
                    DispatchError::from("Failed to draw random tuple from unstructured source")
                })?
                .take(queue_len)
                .for_each(|v| {
                    let (block_num, contract_num, seed) = v.expect("Guaranteed to have value");
                    queue
                        .entry(block_num % (MAX_BLOCK - 2) + MIN_BLOCK) // [MIN_BLOCK..MAX_BLOCK]
                        .or_default()
                        .push((contract_num % num_hashes, Seed(seed)));
                });

            log::debug!(
                "Queue: num entries = {}, num blocks = {}, queue: {:?}",
                queue_len,
                queue.len(),
                queue,
            );

            let mut current_block = 2_u16;

            // check the first block to send a message in is greater than current block
            let mut blocks = queue.keys().cloned();
            if let Some(first_block) = blocks.next() {
                assert!(first_block > current_block);
            }

            // for a block number in [3..N] send out messages and run queue processing
            for (blk, entries) in queue {
                run_to_block_with_ocw(blk as u32, &pool, None);

                for (hash_id, seed) in entries {
                    let author = &accounts
                        .choose(&mut rng)
                        .expect("Accounts vec must not be empty")
                        .0;

                    // Decide whether the message should contain value
                    let value = match rng.next_u32() >> 28 {
                        0_u32 => rng.next_u32() as u128, // 1/16 chances to have value
                        _ => 0_u128,
                    };
                    Gear::send_message(
                        RuntimeOrigin::signed(author.clone()),
                        hashes[hash_id as usize],
                        seed.0.to_vec(),
                        params.gas_limit,
                        value,
                    )
                    .map_err(|e| e.error)?;
                }
                current_block = blk;
            }

            run_to_block_with_ocw((current_block + 11) as u32, &pool, None);

            // Gas balance adds up: all gas is held by waiting messages only
            Ok(GasUsageStats::new(
                GasHandlerOf::<Runtime>::total_supply(),
                total_gas_in_wl_mb(),
                <Runtime as pallet_gear::Config>::Currency::total_issuance(),
                initial_total_balance,
                total_reserved_balance(),
            ))
        })
    } else {
        Err("incompatible params".into())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use demo_compose::WASM_BINARY as COMPOSE_WASM_BINARY;
    use frame_support::assert_ok;

    #[test]
    fn gas_total_supply_is_stable() {
        init_logger();

        let alice = get_account_id_from_seed::<sr25519::Public>("Alice");

        new_test_ext(
            vec![(alice.clone(), 1_000_000_000_000_000_u128)],
            vec!["Val"],
            alice.clone(),
        )
        .execute_with(|| {
            // Initial value in all gas trees is 0
            assert_eq!(GasHandlerOf::<Runtime>::total_supply(), 0);
            assert_eq!(total_gas_in_wl_mb(), 0);

            let composer_id = generate_program_id(NCOMPOSE_WASM_BINARY, b"salt");
            let mul_id = generate_program_id(MUL_CONST_WASM_BINARY, b"salt");

            assert_ok!(Gear::upload_program(
                RuntimeOrigin::signed(alice.clone()),
                MUL_CONST_WASM_BINARY.to_vec(),
                b"salt".to_vec(),
                100_u64.encode(),
                25_000_000_000,
                0,
            ));

            assert_ok!(Gear::upload_program(
                RuntimeOrigin::signed(alice.clone()),
                NCOMPOSE_WASM_BINARY.to_vec(),
                b"salt".to_vec(),
                (<[u8; 32]>::from(mul_id), 8_u16).encode(), // 8 iterations
                25_000_000_000,
                0,
            ));

            run_to_block(2, None);

            assert_ok!(Gear::send_message(
                RuntimeOrigin::signed(alice),
                composer_id,
                10_u64.to_le_bytes().to_vec(),
                100_000_000_000,
                0,
            ));

            run_to_block(4, None);

            let system_reservation = gstd::Config::system_reserve();

            // Gas balance adds up: all gas is held by waiting messages and system reservations only
            assert_eq!(
                GasHandlerOf::<Runtime>::total_supply(),
                total_gas_in_wl_mb() + system_reservation * 8
            );
        });
    }

    #[test]
    fn two_contracts_composition_works() {
        init_logger();
        let alice = get_account_id_from_seed::<sr25519::Public>("Alice");
        new_test_ext(
            vec![(alice.clone(), 1_000_000_000_000_000_u128)],
            vec!["Val"],
            alice.clone(),
        )
        .execute_with(|| {
            // Initial value in all gas trees is 0
            assert_eq!(GasHandlerOf::<Runtime>::total_supply(), 0);
            assert_eq!(total_gas_in_wl_mb(), 0);

            let contract_a_id = generate_program_id(MUL_CONST_WASM_BINARY, b"contract_a");
            let contract_b_id = generate_program_id(MUL_CONST_WASM_BINARY, b"contract_b");
            let compose_id = generate_program_id(COMPOSE_WASM_BINARY, b"salt");

            assert_ok!(Gear::upload_program(
                RuntimeOrigin::signed(alice.clone()),
                MUL_CONST_WASM_BINARY.to_vec(),
                b"contract_a".to_vec(),
                50_u64.encode(),
                2_500_000_000,
                0,
            ));

            assert_ok!(Gear::upload_program(
                RuntimeOrigin::signed(alice.clone()),
                MUL_CONST_WASM_BINARY.to_vec(),
                b"contract_b".to_vec(),
                75_u64.encode(),
                2_500_000_000,
                0,
            ));

            assert_ok!(Gear::upload_program(
                RuntimeOrigin::signed(alice.clone()),
                COMPOSE_WASM_BINARY.to_vec(),
                b"salt".to_vec(),
                (
                    <[u8; 32]>::from(contract_a_id),
                    <[u8; 32]>::from(contract_b_id)
                )
                    .encode(),
                2_500_000_000,
                0,
            ));

            run_to_block(2, None);

            assert_ok!(Gear::send_message(
                RuntimeOrigin::signed(alice.clone()),
                compose_id,
                100_u64.to_le_bytes().to_vec(),
                12_000_000_000,
                0,
            ));

            run_to_block(4, None);

            // Gas balance adds up: all gas is held by waiting messages only
            assert_eq!(
                GasHandlerOf::<Runtime>::total_supply(),
                total_gas_in_wl_mb()
            );
        });
    }

    #[test]
    fn queue_filled_up_ok() {
        let seed = [0_u8; 32];
        let mut rng: StdRng = SeedableRng::from_seed(seed);
        let mut bytes = [0u8; 2 * 36 * MAX_QUEUE_LEN as usize];
        rng.fill_bytes(&mut bytes);

        let u = Unstructured::new(&bytes[..]);

        println!("u.len(): {}", u.len());

        // Generate messages queue
        let queue_len = 10_usize;
        let mut queue = BTreeMap::<u16, Vec<(u16, Seed)>>::new();

        u.arbitrary_take_rest_iter::<(u16, u16, [u8; 32])>()
            .expect("Failed to get arbitrary iter")
            .take(queue_len)
            .for_each(|v| {
                let (block_num, contract_num, seed) = v.expect("Guaranteed to have value");
                queue
                    .entry(block_num % (MAX_BLOCK - 2) + MIN_BLOCK) // [MIN_BLOCK..MAX_BLOCK]
                    .or_default()
                    .push((contract_num % 16, Seed(seed)));
            });

        println!("queue_len: {:?}, queue: {queue:?}", queue.len());
    }

    #[test]
    fn enough_contracts_created() {
        #[cfg(feature = "debug-wasm-mutate")]
        use std::{fmt::Write, fs::File, io::Write as _};

        let _ = env_logger::try_init();

        // Generate test contracts
        let num_contracts = 10_usize;
        let mut contracts = BTreeMap::<ProgramId, (Vec<u8>, Vec<u8>)>::new();

        // Registering the original contract
        let original_code = GENERAL_WASM_BINARY;
        let salt = &0_usize.to_le_bytes()[..];
        let original_contract_id = generate_program_id(original_code, salt);
        contracts.insert(
            original_contract_id,
            (original_code.to_vec(), salt.to_vec()),
        );

        // Attempting to mutate the original wasm code to get as many contracts as necessary
        let mut mutator = WasmMutate::default();
        let mut fuel = 1_000_000_u64;
        mutator.seed(255).preserve_semantics(true);

        let mut count = 1_usize;
        for _ in 0..NUM_TRIES {
            if count >= num_contracts {
                break;
            }
            mutator.fuel(fuel);
            let it = match mutator.run(original_code) {
                Ok(it) => it,
                Err(e) => match e.kind() {
                    ErrorKind::NoMutationsApplicable => continue,
                    ErrorKind::OutOfFuel => {
                        fuel *= 2;
                        continue;
                    }
                    _ => panic!("{}", e),
                },
            };
            for mutated in it.take(num_contracts - 1) {
                let mutated = mutated.expect("Expect mutated wasm");
                let mut validator = Validator::new();
                match validator.validate_all(&mutated[..]) {
                    Ok(_) => {}
                    Err(e) => {
                        println!("Got error {e} where was not supposed to")
                    }
                };

                #[cfg(feature = "debug-wasm-mutate")]
                {
                    let text = wasmprinter::print_bytes(&mutated)
                        .expect("Failed to print wasm bytes into String");
                    let mut path = String::new();
                    write!(path, "contract_{count}.wat").expect("Failed to write to String");
                    let mut output = File::create(path).expect("Failed to create file");
                    write!(output, "{text}").expect("Failed to write to file");
                }

                let salt = &count.to_le_bytes()[..];
                let contract_id = generate_program_id(&mutated[..], salt);
                contracts.insert(contract_id, (mutated, salt.to_vec()));
                count += 1;
                if count >= num_contracts {
                    break;
                }
            }
        }

        #[cfg(feature = "debug-wasm-mutate")]
        {
            let text = wasmprinter::print_bytes(original_code)
                .expect("Failed to print wasm bytes into String");
            let mut output = File::create("contract_0.wat").expect("Failed to create file");
            write!(output, "{text}").expect("Failed to write to file");
        }

        // In case we have fewer contracts than desired, fill the empty slots with the original
        for i in count..num_contracts {
            let salt = &i.to_le_bytes()[..];
            let contract_id = generate_program_id(original_code, salt);
            contracts.insert(contract_id, (original_code.to_vec(), salt.to_vec()));
        }

        println!("num_contracts: {}", contracts.len(),);

        assert_eq!(contracts.len(), 10);
    }

    #[test]
    fn run_target_with_params() {
        let params = crate::SimpleParams {
            num_contracts: 3,
            queue_len: 14,
            gas_limit: 1095299817470,
            seed: 18371870626081079038,
            input: [
                254, 254, 254, 254, 254, 254, 254, 254, 254, 254, 254, 254, 254, 254, 254, 254,
                254, 254, 254, 254, 254, 254, 254, 254, 254, 254, 254, 254, 254, 254, 254, 190,
            ],
        };
        run_target(&crate::Params::Simple(params), simple_scenario);
    }
}
