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

//! Tests for core-processor functionality.

mod utils;

use crate::ProcessorExternalities;
use alloc::vec;
use gear_core::message::DispatchKind;
use utils::{
    Ext, TokenSnapshot, assert_pages_different, assert_pages_equal, assert_pages_populated,
    assert_total_supply_reply, message_sender, run_sequence, run_sequence_without_snapshots,
    step_failure, step_success,
};

#[test]
fn execute_environment_multiple_times_with_memory_replacing() {
    use demo_fungible_token::{FTAction, InitConfig};

    let steps = vec![
        step_success("Init", DispatchKind::Init, InitConfig::test_sequence()),
        step_success("Mint", DispatchKind::Handle, FTAction::Mint(1_000_000)),
        step_failure(
            "Burn too much",
            DispatchKind::Handle,
            FTAction::Burn(2_000_000),
        ),
        step_success(
            "Burn allowed amount",
            DispatchKind::Handle,
            FTAction::Burn(300_000),
        ),
        step_success("TotalSupply", DispatchKind::Handle, FTAction::TotalSupply),
    ];

    let mut snapshot = Ext::memory_snapshot();
    let info = run_sequence(&steps, &mut snapshot);

    // After replaying a multi-step scenario we inspect the raw memory dump
    // and verify the high-level token state matches the business expectations.
    let token_snapshot = TokenSnapshot::from_ext(&info);
    let expected_actor = message_sender();
    assert_eq!(token_snapshot.total_supply, 700_000);
    assert_eq!(token_snapshot.name, "MyToken");
    assert_eq!(token_snapshot.symbol, "MTK");
    assert_eq!(token_snapshot.balances, vec![(expected_actor, 700_000)]);

    assert_pages_populated(&info);
    assert_total_supply_reply(&info);

    let mut snapshot_second = Ext::memory_snapshot();
    let info_second = run_sequence(&steps, &mut snapshot_second);

    let second_snapshot = TokenSnapshot::from_ext(&info_second);
    assert_eq!(token_snapshot, second_snapshot);

    assert_pages_equal(&info, &info_second);
}

#[test]
fn execute_environment_multiple_times_and_compare_results() {
    use demo_fungible_token::{FTAction, InitConfig};

    let steps_with_failure = vec![
        step_success("Init", DispatchKind::Init, InitConfig::test_sequence()),
        step_success("Mint", DispatchKind::Handle, FTAction::Mint(1_000_000)),
        step_failure(
            "Burn too much",
            DispatchKind::Handle,
            FTAction::Burn(2_000_000),
        ),
        step_success(
            "Burn allowed amount",
            DispatchKind::Handle,
            FTAction::Burn(300_000),
        ),
        step_success("TotalSupply", DispatchKind::Handle, FTAction::TotalSupply),
    ];

    let normal_sequence = vec![
        step_success("Init", DispatchKind::Init, InitConfig::test_sequence()),
        step_success("Mint", DispatchKind::Handle, FTAction::Mint(1_000_000)),
        step_success(
            "Burn allowed amount",
            DispatchKind::Handle,
            FTAction::Burn(300_000),
        ),
        step_success("TotalSupply", DispatchKind::Handle, FTAction::TotalSupply),
    ];

    let altered_sequence = vec![
        step_success("Init", DispatchKind::Init, InitConfig::test_sequence()),
        step_success("Mint", DispatchKind::Handle, FTAction::Mint(1_000_000)),
        step_success(
            "Burn different amount",
            DispatchKind::Handle,
            FTAction::Burn(500_000),
        ),
        step_success("TotalSupply", DispatchKind::Handle, FTAction::TotalSupply),
    ];

    let mut snapshot = Ext::memory_snapshot();

    let info_with_failure = run_sequence(&steps_with_failure, &mut snapshot);
    let info_normal = run_sequence(&normal_sequence, &mut snapshot);
    let info_altered = run_sequence(&altered_sequence, &mut snapshot);

    let actor = message_sender();
    let snapshot_with_failure = TokenSnapshot::from_ext(&info_with_failure);
    let snapshot_normal = TokenSnapshot::from_ext(&info_normal);
    let snapshot_altered = TokenSnapshot::from_ext(&info_altered);

    assert_eq!(snapshot_with_failure, snapshot_normal);
    assert_eq!(snapshot_with_failure.total_supply, 700_000);
    assert_eq!(snapshot_altered.total_supply, 500_000);
    assert_eq!(snapshot_altered.balances, vec![(actor, 500_000)]);

    // Failures revert the state, so both sequences should end up with identical memory dumps.
    assert_pages_equal(&info_with_failure, &info_normal);
    // Altering one of the commands yields a different memory snapshot.
    assert_pages_different(&info_normal, &info_altered);

    assert_total_supply_reply(&info_with_failure);
    assert_total_supply_reply(&info_normal);
    assert_total_supply_reply(&info_altered);
}

#[test]
fn execute_sequence_without_snapshots_diverges() {
    use demo_fungible_token::{FTAction, InitConfig};

    let steps = vec![
        step_success("Init", DispatchKind::Init, InitConfig::test_sequence()),
        step_success("Mint", DispatchKind::Handle, FTAction::Mint(1_000_000)),
        step_failure(
            "Burn (should fail)",
            DispatchKind::Handle,
            FTAction::Burn(2_000_000),
        ),
        step_success("TotalSupply", DispatchKind::Handle, FTAction::TotalSupply),
    ];

    let mut snapshot = Ext::memory_snapshot();
    let _ = run_sequence(&steps, &mut snapshot);

    let mut disabled = Ext::memory_snapshot();
    let result = run_sequence_without_snapshots(&steps, &mut disabled);

    assert!(result.is_none(), "Execution without snapshots should fail");
}

#[test]
fn execute_sequence_with_consecutive_failures() {
    use demo_fungible_token::{FTAction, InitConfig};

    let steps = vec![
        step_success("Init", DispatchKind::Init, InitConfig::test_sequence()),
        step_success("Mint", DispatchKind::Handle, FTAction::Mint(1_000_000)),
        step_failure("Burn 1", DispatchKind::Handle, FTAction::Burn(2_000_000)),
        step_failure("Burn 2", DispatchKind::Handle, FTAction::Burn(3_000_000)),
        step_success("Mint again", DispatchKind::Handle, FTAction::Mint(500_000)),
        step_success("TotalSupply", DispatchKind::Handle, FTAction::TotalSupply),
    ];

    let mut snapshot = Ext::memory_snapshot();
    let info = run_sequence(&steps, &mut snapshot);

    // Consecutive failures should not leak state across retries; once the run succeeds we confirm the final balances.
    let token_snapshot = TokenSnapshot::from_ext(&info);
    let actor = message_sender();
    assert_eq!(token_snapshot.total_supply, 1_500_000);
    assert_eq!(token_snapshot.balances, vec![(actor, 1_500_000)]);

    assert_pages_populated(&info);
    assert_total_supply_reply(&info);
}
