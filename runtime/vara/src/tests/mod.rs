// This file is part of Gear.

// Copyright (C) 2021-2025 Gear Technologies Inc.
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

//! Weight tests for the runtime.

use super::*;
use crate::Runtime;
use frame_support::dispatch::GetDispatchInfo;
use frame_system::limits::WeightsPerClass;
use gear_core::costs::{
    DbCosts, InstantiationCosts, InstrumentationCosts, IoCosts, LazyPagesCosts, PagesCosts,
    ProcessCosts,
};
use pallet_gear::{
    DbWeights, InstantiationWeights, InstructionWeights, InstrumentationWeights, MemoryWeights,
    Schedule, SyscallWeights,
};
use pallet_staking::WeightInfo as _;
use sp_runtime::AccountId32;

use pallet_gear_builtin::BuiltinActor;
#[cfg(feature = "dev")]
use sp_runtime::traits::AccountIdConversion;

mod utils;

use utils::*;

#[cfg(feature = "dev")]
#[test]
fn bridge_storages_have_correct_prefixes() {
    // # SAFETY: Do not change storage prefixes without total bridge re-deploy.
    const PALLET_PREFIX: &str = "GearEthBridge";

    const AUTHORITY_SET_HASH_STORAGE_PREFIX: &str = "AuthoritySetHash";
    const AUTHORITY_SET_HASH_PREFIX_HASH: [u8; 32] = [
        253, 110, 2, 127, 122, 27, 216, 186, 166, 64, 108, 234, 77, 128, 217, 50, 113, 32, 253, 42,
        221, 109, 18, 73, 191, 27, 107, 252, 59, 223, 81, 15,
    ];

    const QUEUE_MERKLE_ROOT_STORAGE_PREFIX: &str = "QueueMerkleRoot";
    const QUEUE_MERKLE_ROOT_PREFIX_HASH: [u8; 32] = [
        253, 110, 2, 127, 122, 27, 216, 186, 166, 64, 108, 234, 77, 128, 217, 50, 223, 80, 147, 16,
        188, 101, 91, 191, 117, 165, 181, 99, 252, 60, 142, 238,
    ];

    assert_eq!(
        pallet_gear_eth_bridge::Pallet::<Runtime>::authority_set_hash_storage_info(),
        (
            PALLET_PREFIX,
            AUTHORITY_SET_HASH_STORAGE_PREFIX,
            AUTHORITY_SET_HASH_PREFIX_HASH
        )
    );

    assert_eq!(
        pallet_gear_eth_bridge::Pallet::<Runtime>::queue_merkle_root_storage_info(),
        (
            PALLET_PREFIX,
            QUEUE_MERKLE_ROOT_STORAGE_PREFIX,
            QUEUE_MERKLE_ROOT_PREFIX_HASH
        )
    );
}

#[cfg(feature = "dev")]
#[test]
fn bridge_accounts_check() {
    // # SAFETY: Do not change bridge pallet id without check of
    // correct integration with already running network.
    //
    // Change of the pallet id will require migrating `pallet-gear-eth-bridge`'s
    // `BridgeAdmin` and `BridgePauser` constants if they are derived from it.
    // We explicitly use the *original* intended PalletId here for the check.
    let original_pallet_id = PalletId(*b"py/gethb");
    let expected_admin_account: AccountId =
        original_pallet_id.into_sub_account_truncating("bridge_admin");
    let expected_pauser_account: AccountId =
        original_pallet_id.into_sub_account_truncating("bridge_pauser");

    // Check if the constants defined in lib.rs match the expected derived accounts.
    assert_eq!(
        GearEthBridgeAdminAccount::get(),
        expected_admin_account,
        "BridgeAdmin constant does not match expected derivation from PalletId"
    );
    assert_eq!(
        GearEthBridgePauserAccount::get(),
        expected_pauser_account,
        "BridgePauser constant does not match expected derivation from PalletId"
    );
}

#[test]
fn payout_stakers_fits_in_block() {
    let expected_weight =
        <Runtime as pallet_staking::Config>::WeightInfo::payout_stakers_alive_staked(
            <Runtime as pallet_staking::Config>::MaxExposurePageSize::get(),
        );

    let call: <Runtime as frame_system::Config>::RuntimeCall =
        RuntimeCall::Staking(pallet_staking::Call::payout_stakers {
            validator_stash: AccountId32::new(Default::default()),
            era: Default::default(),
        });

    let dispatch_info = call.get_dispatch_info();

    assert_eq!(dispatch_info.class, DispatchClass::Normal);
    assert_eq!(dispatch_info.weight, expected_weight);

    let block_weights = <Runtime as frame_system::Config>::BlockWeights::get();

    let normal_class_weights: WeightsPerClass =
        block_weights.per_class.get(DispatchClass::Normal).clone();

    let normal_ref_time = normal_class_weights
        .max_extrinsic
        .unwrap_or(Weight::MAX)
        .ref_time();
    let base_weight = normal_class_weights.base_extrinsic.ref_time();

    assert!(normal_ref_time - base_weight > expected_weight.ref_time());
}

#[test]
fn normal_dispatch_length_suits_minimal() {
    const MB: u32 = 1024 * 1024;

    let block_length = <Runtime as frame_system::Config>::BlockLength::get();

    // Normal dispatch class is bigger than 2 MB.
    assert!(*block_length.max.get(DispatchClass::Normal) > 2 * MB);

    // Others are on maximum.
    assert_eq!(*block_length.max.get(DispatchClass::Operational), 5 * MB);
    assert_eq!(*block_length.max.get(DispatchClass::Mandatory), 5 * MB);
}

#[test]
fn instruction_weights_heuristics_test() {
    let weights = InstructionWeights::<Runtime>::default();

    let expected_weights = InstructionWeights {
        version: 0,
        _phantom: core::marker::PhantomData,

        i64const: 160,
        i64load: 5_800,
        i32load: 8_000,
        i64store: 10_000,
        i32store: 10_000,
        select: 7_100,
        r#if: 8_000,
        br: 3_300,
        br_if: 6_000,
        br_table: 10_900,
        br_table_per_entry: 150,

        call: 4_900,
        call_per_local: 0,
        call_indirect: 22_100,
        call_indirect_per_param: 1_000,

        local_get: 900,
        local_set: 1_900,
        local_tee: 2_500,
        global_get: 700,
        global_set: 1_000,
        memory_current: 14_200,

        i64clz: 600,
        i32clz: 300,
        i64ctz: 610,
        i32ctz: 250,
        i64popcnt: 450,
        i32popcnt: 350,
        i64eqz: 2_100,
        i32eqz: 1_200,
        i32extend8s: 200,
        i32extend16s: 200,
        i64extend8s: 400,
        i64extend16s: 400,
        i64extend32s: 400,
        i64extendsi32: 200,
        i64extendui32: 200,
        i32wrapi64: 200,
        i64eq: 1_800,
        i32eq: 1_100,
        i64ne: 1_700,
        i32ne: 1_000,

        i64lts: 1_200,
        i32lts: 1_000,
        i64ltu: 1_800,
        i32ltu: 1_000,
        i64gts: 1_800,
        i32gts: 1_000,
        i64gtu: 1_900,
        i32gtu: 1_000,
        i64les: 1_900,
        i32les: 1_000,
        i64leu: 1_200,
        i32leu: 1_000,

        i64ges: 1_300,
        i32ges: 1_000,
        i64geu: 1_300,
        i32geu: 1_000,
        i64add: 1_300,
        i32add: 500,
        i64sub: 1_300,
        i32sub: 500,
        i64mul: 2_000,
        i32mul: 1_000,
        i64divs: 3_500,
        i32divs: 3_500,

        i64divu: 3_500,
        i32divu: 3_500,
        i64rems: 18_000,
        i32rems: 15_000,
        i64remu: 3_500,
        i32remu: 3_500,
        i64and: 1_000,
        i32and: 500,
        i64or: 1_000,
        i32or: 500,
        i64xor: 1_000,
        i32xor: 500,

        i64shl: 1_000,
        i32shl: 200,
        i64shrs: 1_000,
        i32shrs: 250,
        i64shru: 1_000,
        i32shru: 400,
        i64rotl: 750,
        i32rotl: 400,
        i64rotr: 1_000,
        i32rotr: 300,
    };

    let result = check_instructions_weights(weights, expected_weights);

    assert!(result.is_ok(), "{:#?}", result.err().unwrap());
    assert_eq!(result.unwrap(), expected_instructions_weights_count());
}

#[test]
fn syscall_weights_test() {
    let weights = SyscallWeights::<Runtime>::default();

    let expected = SyscallWeights {
        alloc: 1_800_000.into(),
        free: 1_000_000.into(),
        free_range: 1_100_000.into(),
        free_range_per_page: 50_000.into(),
        gr_reserve_gas: 2_600_000.into(),
        gr_unreserve_gas: 2_600_000.into(),
        gr_system_reserve_gas: 1_200_000.into(),
        gr_gas_available: 1_200_000.into(),
        gr_message_id: 1_200_000.into(),
        gr_program_id: 1_200_000.into(),
        gr_source: 1_200_000.into(),
        gr_value: 1_200_000.into(),
        gr_value_available: 1_200_000.into(),
        gr_size: 1_200_000.into(),
        gr_read: 1_800_000.into(),
        gr_read_per_byte: 200.into(),
        gr_env_vars: 1_200_000.into(),
        gr_block_height: 1_200_000.into(),
        gr_block_timestamp: 1_200_000.into(),
        gr_random: 2_300_000.into(),
        gr_reply_deposit: 6_000_000.into(),
        gr_send: 3_200_000.into(),
        gr_send_per_byte: 550.into(),
        gr_send_wgas: 3_300_000.into(),
        gr_send_wgas_per_byte: 550.into(),
        gr_send_init: 1_300_000.into(),
        gr_send_push: 2_050_000.into(),
        gr_send_push_per_byte: 550.into(),
        gr_send_commit: 2_700_000.into(),
        gr_send_commit_wgas: 2_800_000.into(),
        gr_reservation_send: 4_000_000.into(),
        gr_reservation_send_per_byte: 500.into(),
        gr_reservation_send_commit: 3_300_000.into(),
        gr_reply_commit: 13_000_000.into(),
        gr_reply_commit_wgas: 12_000_000.into(),
        gr_reservation_reply: 9_500_000.into(),
        gr_reservation_reply_per_byte: 800.into(),
        gr_reservation_reply_commit: 8_000_000.into(),
        gr_reply_push: 2_000_000.into(),
        gr_reply: 14_500_000.into(),
        gr_reply_per_byte: 750.into(),
        gr_reply_wgas: 13_500_000.into(),
        gr_reply_wgas_per_byte: 750.into(),
        gr_reply_push_per_byte: 740.into(),
        gr_reply_to: 1_200_000.into(),
        gr_signal_code: 1_200_000.into(),
        gr_signal_from: 1_200_000.into(),
        gr_reply_input: 11_000_000.into(),
        gr_reply_input_wgas: 13_000_000.into(),
        gr_reply_push_input: 1_400_000.into(),
        gr_reply_push_input_per_byte: 130.into(),
        gr_send_input: 3_100_000.into(),
        gr_send_input_wgas: 3_400_000.into(),
        gr_send_push_input: 1_500_000.into(),
        gr_send_push_input_per_byte: 150.into(),
        gr_debug: 1_300_000.into(),
        gr_debug_per_byte: 500.into(),
        gr_reply_code: 1_200_000.into(),
        gr_exit: 21_000_000.into(),
        gr_leave: 17_000_000.into(),
        gr_wait: 16_000_000.into(),
        gr_wait_for: 16_000_000.into(),
        gr_wait_up_to: 16_500_000.into(),
        gr_wake: 3_300_000.into(),
        gr_create_program: 4_100_000.into(),
        gr_create_program_payload_per_byte: 120.into(),
        gr_create_program_salt_per_byte: 1_600.into(),
        gr_create_program_wgas: 4_100_000.into(),
        gr_create_program_wgas_payload_per_byte: 130.into(),
        gr_create_program_wgas_salt_per_byte: 1_500.into(),
        _phantom: Default::default(),
    };

    let result = check_syscall_weights(weights, expected);

    assert!(result.is_ok(), "{:#?}", result.err().unwrap());
    assert_eq!(result.unwrap(), expected_syscall_weights_count());
}

#[test]
fn page_costs_heuristic_test() {
    let io_costs: IoCosts = MemoryWeights::<Runtime>::default().into();

    let expected_page_costs = PagesCosts {
        load_page_data: 10_000_000.into(),
        upload_page_data: 105_000_000.into(),
        mem_grow: 700_000.into(),
        mem_grow_per_page: 3.into(),
        parachain_read_heuristic: 0.into(),
    };

    let result = check_pages_costs(io_costs.common, expected_page_costs);

    assert!(result.is_ok(), "{:#?}", result.err().unwrap());
    assert_eq!(result.unwrap(), expected_pages_costs_count());
}

#[test]
fn lazy_page_costs_heuristic_test() {
    let io_costs: IoCosts = MemoryWeights::<Runtime>::default().into();

    let expected_lazy_pages_costs = LazyPagesCosts {
        signal_read: 28_000_000.into(),
        signal_write: 138_000_000.into(),
        signal_write_after_read: 112_000_000.into(),
        host_func_read: 29_000_000.into(),
        host_func_write: 137_000_000.into(),
        host_func_write_after_read: 112_000_000.into(),
        load_page_storage_data: 10_000_000.into(),
    };

    let result = check_lazy_pages_costs(io_costs.lazy_pages, expected_lazy_pages_costs);

    assert!(result.is_ok(), "{:#?}", result.err().unwrap());
    assert_eq!(result.unwrap(), expected_lazy_pages_costs_count());
}

#[test]
fn load_allocations_costs_heuristic_test() {
    let process_costs: ProcessCosts = Schedule::<Runtime>::default().process_costs();

    let expected_load_allocations_costs = 25197;

    let result = check_load_allocations_costs(
        process_costs.load_allocations_per_interval.into(),
        expected_load_allocations_costs,
    );

    assert!(result.is_ok(), "{:#?}", result.err().unwrap());
    assert_eq!(result.unwrap(), expected_load_allocations_costs_count());
}

#[test]
fn instantiation_costs_heuristic_test() {
    let instantiation_costs = InstantiationWeights::<Runtime>::default().into();

    let expected_instantiation_costs = InstantiationCosts {
        code_section_per_byte: 2582.into(),
        data_section_per_byte: 661.into(),
        global_section_per_byte: 3000.into(),
        table_section_per_byte: 651.into(),
        element_section_per_byte: 2523.into(),
        type_section_per_byte: 22356.into(),
    };

    let result = check_instantiation_costs(instantiation_costs, expected_instantiation_costs);

    assert!(result.is_ok(), "{:#?}", result.err().unwrap());
    assert_eq!(result.unwrap(), expected_instantiation_costs_count());
}

#[test]
fn db_costs_heuristic_test() {
    let db_costs = DbWeights::<Runtime>::default().into();

    let expected_db_costs = DbCosts {
        read: 25000000.into(),
        read_per_byte: 800.into(),
        write: 100000000.into(),
        write_per_byte: 250.into(),
    };

    let result = check_db_costs(db_costs, expected_db_costs);

    assert!(result.is_ok(), "{:#?}", result.err().unwrap());
    assert_eq!(result.unwrap(), expected_db_costs_count());
}

#[test]
fn code_instrumentation_costs_heuristic_test() {
    let code_instrumentation_costs = InstrumentationWeights::<Runtime>::default().into();

    let expected_code_instrumentation_costs = InstrumentationCosts {
        base: 412026293.into(),
        per_byte: 715243.into(),
    };

    let result = check_code_instrumentation_costs(
        code_instrumentation_costs,
        expected_code_instrumentation_costs,
    );

    assert!(result.is_ok(), "{:#?}", result.err().unwrap());
    assert_eq!(result.unwrap(), expected_code_instrumentation_costs_count());
}

/// Check that it is not possible to write/change memory pages too cheaply,
/// because this may cause runtime heap memory overflow.
#[test]
fn write_is_not_too_cheap() {
    let costs: IoCosts = MemoryWeights::<Runtime>::default().into();

    #[allow(clippy::unnecessary_min_or_max)]
    let cheapest_write = u64::MAX
        .min(costs.lazy_pages.signal_write.cost_for_one())
        .min(
            costs.lazy_pages.signal_read.cost_for_one()
                + costs.lazy_pages.signal_write_after_read.cost_for_one(),
        )
        .min(costs.lazy_pages.host_func_write.cost_for_one())
        .min(
            costs.lazy_pages.host_func_read.cost_for_one()
                + costs.lazy_pages.host_func_write_after_read.cost_for_one(),
        );

    let block_max_gas = 3 * (10 ^ 12); // 3 seconds
    let runtime_heap_size_in_wasm_pages = 0x4000; // 1GB

    assert!((block_max_gas / cheapest_write) < runtime_heap_size_in_wasm_pages);
}

#[cfg(feature = "dev")]
#[test]
fn eth_bridge_builtin_id_matches() {
    use common::Origin;

    assert_eq!(
        GearBuiltin::builtin_id_into_actor_id(
            <pallet_gear_eth_bridge::builtin::Actor<Runtime> as BuiltinActor>::TYPE.id()
        )
        .cast::<AccountId>(),
        <Runtime as pallet_gear_eth_bridge::Config>::BuiltinAddress::get(),
    )
}
