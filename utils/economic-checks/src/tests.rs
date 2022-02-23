// This file is part of Gear.

// Copyright (C) 2021 Gear Technologies Inc.
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

use codec::{Decode, Encode};
use common::{self, DAGBasedLedger, Dispatch};
use frame_support::{assert_ok, dispatch::DispatchError};
use gear_runtime::{Gear, Origin, Runtime};
use hex_literal::hex;
use primitive_types::H256;
use quickcheck::{quickcheck, Arbitrary, Gen, TestResult};
use sp_std::collections::btree_map::BTreeMap;
use tests_compose::WASM_BINARY_BLOATY as COMPOSE_WASM_BINARY;
use tests_mul_by_const::WASM_BINARY_BLOATY as MUL_CONST_WASM_BINARY;
use tests_ncompose::WASM_BINARY_BLOATY as NCOMPOSE_WASM_BINARY;

use crate::util::*;

const ALICE: [u8; 32] = hex!["0100000000000000000000000000000000000000000000000000000000000000"];

type GasNodeKeyOf<T> = <<T as pallet_gear::Config>::GasHandler as DAGBasedLedger>::Key;
type GasBalanceOf<T> = <<T as pallet_gear::Config>::GasHandler as DAGBasedLedger>::Balance;

fn total_gas_in_wait_list() -> u64 {
    // Iterate through the wait list and record the respective gas nodes value limits
    // attributing the latter to the nearest `node_with_value` ID to avoid duplication
    let specified_value_by_node_id: BTreeMap<GasNodeKeyOf<Runtime>, GasBalanceOf<Runtime>> =
        frame_support::storage::PrefixIterator::<(u64, H256)>::new(
            common::STORAGE_WAITLIST_PREFIX.to_vec(),
            common::STORAGE_WAITLIST_PREFIX.to_vec(),
            |_, mut value| {
                let (dispatch, _) = <(Dispatch, u32)>::decode(&mut value)?;
                Ok(
                    <Runtime as pallet_gear::Config>::GasHandler::get_limit(dispatch.message.id)
                        .expect("Waitlisted messages must have associated gas"),
                )
            },
        )
        .map(|(gas, node_id)| (node_id, gas))
        .collect();

    specified_value_by_node_id
        .into_iter()
        .fold(0_u64, |acc, (_, val)| acc + val)
}

#[derive(Debug, Clone)]
struct Params {
    depth: u16,
    intrinsic_value: u64,
    gas_limit: u64,
}

impl Arbitrary for Params {
    fn arbitrary(gen: &mut Gen) -> Self {
        Self {
            depth: <u16>::arbitrary(gen) / 64, // `depth` param varies within [0..1024] range
            intrinsic_value: 100 + <u64>::arbitrary(gen) / (1024 * 1024 * 1024), // roughly [10^2..17*10^9]
            gas_limit: 10_000_000_u64 + <u64>::arbitrary(gen) / (16 * 1024 * 1024), // roughly [10^7..10^12]
        }
    }
}

#[derive(Default, Debug)]
struct TestOutcome {
    total_gas_supply: u64,
    accounted: u64,
}

impl TestOutcome {
    fn new(total: u64, accounted: u64) -> Self {
        Self {
            total_gas_supply: total,
            accounted: accounted,
        }
    }
}

fn check_gas_consistency(params: &Params) -> Result<TestOutcome, DispatchError> {
    let (mut ext, pool) = with_offchain_ext(vec![(ALICE, 1_000_000_000_000_000_u128)]);
    ext.execute_with(|| {
        // Initial value in all gas trees is 0
        if <Runtime as pallet_gear::Config>::GasHandler::total_supply() != 0
            || total_gas_in_wait_list() != 0
        {
            return Ok(TestOutcome::new(
                <Runtime as pallet_gear::Config>::GasHandler::total_supply(),
                total_gas_in_wait_list(),
            ));
        }

        let composer_id =
            generate_program_id(NCOMPOSE_WASM_BINARY.expect("Wasm binary missing!"), b"salt");
        let mul_id = generate_program_id(
            MUL_CONST_WASM_BINARY.expect("Wasm binary missing!"),
            b"salt",
        );

        Gear::submit_program(
            Origin::signed(ALICE.into()).into(),
            MUL_CONST_WASM_BINARY
                .expect("Wasm binary missing!")
                .to_vec(),
            b"salt".to_vec(),
            params.intrinsic_value.encode(),
            30_000_000,
            0,
        )
        .map_err(|e| e.error)?;

        Gear::submit_program(
            Origin::signed(ALICE.into()).into(),
            NCOMPOSE_WASM_BINARY.expect("Wasm binary missing!").to_vec(),
            b"salt".to_vec(),
            (<[u8; 32]>::from(mul_id), params.depth).encode(),
            50_000_000,
            0,
        )
        .map_err(|e| e.error)?;

        run_to_block_with_ocw(2, pool.clone(), None);

        Gear::send_message(
            Origin::signed(ALICE.into()).into(),
            composer_id,
            1_u64.to_le_bytes().to_vec(),
            params.gas_limit,
            0,
        )
        .map_err(|e| e.error)?;

        // Modeling offchain workers being run every certain number of blocks
        run_to_block_with_ocw(50, pool.clone(), None);

        log::debug!(
            "Gas held by waitlisted messages: {:?}",
            total_gas_in_wait_list()
        );

        // Gas balance adds up: all gas is held by waiting messages only
        Ok(TestOutcome::new(
            <Runtime as pallet_gear::Config>::GasHandler::total_supply(),
            total_gas_in_wait_list(),
        ))
    })
}

quickcheck! {
    fn chain_of_multiplications(params: Params) -> TestResult {
        init_logger();
        log::debug!("[quickcheck::chain_of_multiplications] params = {:?}", &params);
        match check_gas_consistency(&params) {
            Ok(outcome) => {
                log::debug!("[quickcheck::chain_of_multiplications] test outcome = {:?}", &outcome);
                TestResult::from_bool(outcome.total_gas_supply == outcome.accounted)
            },
            _ => TestResult::discard()
        }
    }
}

#[test]
fn gas_total_supply_is_stable() {
    init_logger();
    new_test_ext(vec![(ALICE, 1_000_000_000_000_000_u128)]).execute_with(|| {
        // Initial value in all gas trees is 0
        assert_eq!(
            <Runtime as pallet_gear::Config>::GasHandler::total_supply(),
            0
        );
        assert_eq!(total_gas_in_wait_list(), 0);

        let composer_id =
            generate_program_id(NCOMPOSE_WASM_BINARY.expect("Wasm binary missing!"), b"salt");
        let mul_id = generate_program_id(
            MUL_CONST_WASM_BINARY.expect("Wasm binary missing!"),
            b"salt",
        );

        assert_ok!(Gear::submit_program(
            Origin::signed(ALICE.into()).into(),
            MUL_CONST_WASM_BINARY
                .expect("Wasm binary missing!")
                .to_vec(),
            b"salt".to_vec(),
            100_u64.encode(),
            30_000_000,
            0,
        ));

        assert_ok!(Gear::submit_program(
            Origin::signed(ALICE.into()).into(),
            NCOMPOSE_WASM_BINARY.expect("Wasm binary missing!").to_vec(),
            b"salt".to_vec(),
            (<[u8; 32]>::from(mul_id), 8_u16).encode(), // 8 iterations
            50_000_000,
            0,
        ));

        run_to_block(2, None);

        assert_ok!(Gear::send_message(
            Origin::signed(ALICE.into()).into(),
            composer_id,
            10_u64.to_le_bytes().to_vec(),
            100_000_000_000,
            0,
        ));

        run_to_block(4, None);

        log::debug!(
            "Gas held by waitlisted messages: {:?}",
            total_gas_in_wait_list()
        );

        // Gas balance adds up: all gas is held by waiting messages only
        assert_eq!(
            <Runtime as pallet_gear::Config>::GasHandler::total_supply(),
            total_gas_in_wait_list()
        );
    });
}

#[test]
fn two_contracts_composition_works() {
    init_logger();
    new_test_ext(vec![(ALICE, 1_000_000_000_000_000_u128)]).execute_with(|| {
        // Initial value in all gas trees is 0
        assert_eq!(
            <Runtime as pallet_gear::Config>::GasHandler::total_supply(),
            0
        );
        assert_eq!(total_gas_in_wait_list(), 0);

        let contract_a_id = generate_program_id(
            MUL_CONST_WASM_BINARY.expect("Wasm binary missing!"),
            b"contract_a",
        );
        let contract_b_id = generate_program_id(
            MUL_CONST_WASM_BINARY.expect("Wasm binary missing!"),
            b"contract_b",
        );
        let compose_id =
            generate_program_id(COMPOSE_WASM_BINARY.expect("Wasm binary missing!"), b"salt");

        assert_ok!(Gear::submit_program(
            Origin::signed(ALICE.into()).into(),
            MUL_CONST_WASM_BINARY
                .expect("Wasm binary missing!")
                .to_vec(),
            b"contract_a".to_vec(),
            50_u64.encode(),
            30_000_000,
            0,
        ));

        assert_ok!(Gear::submit_program(
            Origin::signed(ALICE.into()).into(),
            MUL_CONST_WASM_BINARY
                .expect("Wasm binary missing!")
                .to_vec(),
            b"contract_b".to_vec(),
            75_u64.encode(),
            30_000_000,
            0,
        ));

        assert_ok!(Gear::submit_program(
            Origin::signed(ALICE.into()).into(),
            COMPOSE_WASM_BINARY.expect("Wasm binary missing!").to_vec(),
            b"salt".to_vec(),
            (
                <[u8; 32]>::from(contract_a_id),
                <[u8; 32]>::from(contract_b_id)
            )
                .encode(),
            40_000_000,
            0,
        ));

        run_to_block(2, None);

        assert_ok!(Gear::send_message(
            Origin::signed(ALICE.into()).into(),
            compose_id,
            100_u64.to_le_bytes().to_vec(),
            100_000_000_000,
            0,
        ));

        run_to_block(4, None);

        log::debug!(
            "Gas held by waitlisted messages: {:?}",
            total_gas_in_wait_list()
        );

        // Gas balance adds up: all gas is held by waiting messages only
        assert_eq!(
            <Runtime as pallet_gear::Config>::GasHandler::total_supply(),
            total_gas_in_wait_list()
        );
    });
}
