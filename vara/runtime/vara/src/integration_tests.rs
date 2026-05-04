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

use crate::*;
use frame_support::{
    assert_noop, assert_ok,
    traits::{
        LockableCurrency, OnFinalize, OnInitialize, WithdrawReasons, fungible,
        tokens::{DepositConsequence, Fortitude, Preservation, Provenance},
    },
};
use frame_system::pallet_prelude::BlockNumberFor;
use gear_core::gas_metering::CustomConstantCostRules;
use gear_wasm_instrument::{BlockType, Instruction, MemArg, Module, Rules};
use sp_consensus_babe::{
    BABE_ENGINE_ID, Slot,
    digests::{PreDigest, SecondaryPlainPreDigest},
};
use sp_core::{Pair, ed25519, sr25519};
use sp_keyring::AccountKeyring;
use sp_runtime::{BuildStorage, Digest, DigestItem};

const ENDOWMENT: u128 = 100_000 * UNITS;
const STASH: u128 = 1_000 * UNITS;

pub(crate) fn initialize_block(new_blk: BlockNumberFor<Runtime>) {
    // All blocks are to be authored by validator at index 0
    System::initialize(
        &new_blk,
        &System::parent_hash(),
        &Digest {
            logs: vec![DigestItem::PreRuntime(
                BABE_ENGINE_ID,
                PreDigest::SecondaryPlain(SecondaryPlainPreDigest {
                    slot: Slot::from(u64::from(new_blk)),
                    authority_index: 0,
                })
                .encode(),
            )],
        },
    );
    System::set_block_number(new_blk);
}

pub(crate) fn on_initialize(new_block_number: BlockNumberFor<Runtime>) {
    System::on_initialize(new_block_number);
    Babe::on_initialize(new_block_number);
    Balances::on_initialize(new_block_number);
    Authorship::on_initialize(new_block_number);
    Treasury::on_initialize(new_block_number);
    GearProgram::on_initialize(new_block_number);
    GearMessenger::on_initialize(new_block_number);
    Gear::on_initialize(new_block_number);
    GearBank::on_initialize(new_block_number);
    GearGas::on_initialize(new_block_number);
    // Session::on_initialize(new_block_number);
}

// Run on_finalize hooks (in pallets reverse order, as they appear in AllPalletsWithSystem)
pub(crate) fn on_finalize(current_blk: BlockNumberFor<Runtime>) {
    Gear::run(frame_support::dispatch::RawOrigin::None.into(), None).unwrap();
    GearPayment::on_finalize(current_blk);
    GearGas::on_finalize(current_blk);
    Gear::on_finalize(current_blk);
    GearBank::on_finalize(current_blk);
    GearMessenger::on_finalize(current_blk);
    GearProgram::on_finalize(current_blk);
    Treasury::on_finalize(current_blk);
    Authorship::on_finalize(current_blk);
    Balances::on_finalize(current_blk);
    Grandpa::on_finalize(current_blk);
    Babe::on_finalize(current_blk);
    System::on_finalize(current_blk);
}

// (stash_acc_id, controller_acc_id, babe_id, grandpa_id, imonline_id, authority_discovery_id)
pub type ValidatorAccountId = (
    AccountId,
    AccountId,
    sr25519::Public,
    ed25519::Public,
    sr25519::Public,
    sr25519::Public,
);

// (who, vesting_start_block, vesting_duration, unfrozen_balance)
type VestingInfo = (AccountId, BlockNumber, BlockNumber, Balance);

#[derive(Default)]
pub struct ExtBuilder {
    initial_authorities: Vec<ValidatorAccountId>,
    stash: u128,
    endowment: Balance,
    endowed_accounts: Vec<AccountId>,
    vested_accounts: Vec<VestingInfo>,
    root: Option<AccountId>,
}

impl ExtBuilder {
    pub fn stash(mut self, s: u128) -> Self {
        self.stash = s;
        self
    }

    pub fn endowment(mut self, s: Balance) -> Self {
        self.endowment = s;
        self
    }

    pub fn initial_authorities(mut self, authorities: Vec<ValidatorAccountId>) -> Self {
        self.initial_authorities = authorities;
        self
    }

    pub fn endowed_accounts(mut self, accounts: Vec<AccountId>) -> Self {
        self.endowed_accounts = accounts;
        self
    }

    pub fn vested_accounts(mut self, vesting: Vec<VestingInfo>) -> Self {
        self.vested_accounts = vesting;
        self
    }

    pub fn root(mut self, a: AccountId) -> Self {
        self.root = Some(a);
        self
    }

    pub fn build(self) -> sp_io::TestExternalities {
        let mut storage = frame_system::GenesisConfig::<Runtime>::default()
            .build_storage()
            .unwrap();

        let mut balances = self
            .initial_authorities
            .iter()
            .map(|x| (x.0.clone(), self.stash))
            .chain(
                self.endowed_accounts
                    .iter()
                    .map(|k| (k.clone(), self.endowment)),
            )
            .collect::<Vec<_>>();

        balances.push((GearBank::bank_address(), EXISTENTIAL_DEPOSIT));

        pallet_balances::GenesisConfig::<Runtime> { balances }
            .assimilate_storage(&mut storage)
            .unwrap();

        SessionConfig {
            keys: self
                .initial_authorities
                .iter()
                .map(|x| {
                    (
                        x.0.clone(),
                        x.0.clone(),
                        SessionKeys {
                            babe: x.2.into(),
                            grandpa: x.3.into(),
                            im_online: x.4.into(),
                            authority_discovery: x.5.into(),
                        },
                    )
                })
                .collect(),
            ..Default::default()
        }
        .assimilate_storage(&mut storage)
        .unwrap();

        #[cfg(feature = "dev")]
        SudoConfig { key: self.root }
            .assimilate_storage(&mut storage)
            .unwrap();

        TreasuryConfig::default()
            .assimilate_storage(&mut storage)
            .unwrap();

        VestingConfig {
            vesting: self.vested_accounts,
        }
        .assimilate_storage(&mut storage)
        .unwrap();

        let mut ext: sp_io::TestExternalities = storage.into();

        ext.execute_with(|| {
            let new_blk = 1;
            initialize_block(new_blk);
            on_initialize(new_blk);
        });
        ext
    }
}

#[allow(unused)]
pub(crate) fn run_to_block(n: u32) {
    while System::block_number() < n {
        let current_blk = System::block_number();
        on_finalize(current_blk);

        let new_block_number = current_blk + 1;
        initialize_block(new_block_number);
        on_initialize(new_block_number);
    }
}

#[allow(unused)]
pub(crate) fn run_for_n_blocks(n: u32) {
    let now = System::block_number();
    let until = now + n;
    for current_blk in now..until {
        on_finalize(current_blk);

        let new_block_number = current_blk + 1;
        initialize_block(new_block_number);
        on_initialize(new_block_number);
    }
}

pub(crate) fn init_logger() {
    let _ = tracing_subscriber::fmt::try_init();
}

pub(crate) fn get_last_program_id() -> [u8; 32] {
    let event = match System::events().last().map(|r| r.event.clone()) {
        Some(RuntimeEvent::Gear(e)) => e,
        _ => unreachable!("Expecting a Gear event"),
    };

    if let pallet_gear::Event::MessageQueued { destination, .. } = event {
        destination.into()
    } else {
        unreachable!("expect RuntimeEvent::InitMessageEnqueued")
    }
}

pub(crate) fn get_treasury_events() -> (Balance, Balance, Balance) {
    System::events()
        .into_iter()
        .fold((0, 0, 0), |r, e| match e.event {
            RuntimeEvent::Treasury(pallet_treasury::Event::Spending { budget_remaining }) => {
                (budget_remaining, r.1, r.2)
            }
            RuntimeEvent::Treasury(pallet_treasury::Event::Burnt { burnt_funds }) => {
                (r.0, burnt_funds, r.2)
            }
            RuntimeEvent::Treasury(pallet_treasury::Event::Rollover { rollover_balance }) => {
                (r.0, r.1, rollover_balance)
            }
            _ => r,
        })
}

#[test]
fn tokens_locking_works() {
    init_logger();

    let wasm_module = r#"
    (module
        (import "env" "memory" (memory 1))
        (export "handle" (func $handle))
        (export "init" (func $init))
        (func $handle)
        (func $init)
    )"#;
    let code = wat::parse_str(wasm_module).unwrap();
    let alice = AccountKeyring::Alice;
    let bob = AccountKeyring::Bob;
    let charlie = AccountKeyring::Charlie;
    let dave = AccountKeyring::Dave;
    let eve = AccountKeyring::Eve;
    let ferdie = AccountKeyring::Ferdie;

    ExtBuilder::default()
        .initial_authorities(vec![
            (
                alice.into(),
                charlie.into(),
                alice.public(),
                ed25519::Pair::from_string("//Alice", None)
                    .unwrap()
                    .public(),
                alice.public(),
                alice.public(),
            ),
            (
                bob.into(),
                dave.into(),
                bob.public(),
                ed25519::Pair::from_string("//Bob", None).unwrap().public(),
                bob.public(),
                bob.public(),
            ),
        ])
        .stash(STASH)
        .endowment(ENDOWMENT)
        .endowed_accounts(vec![charlie.into(), dave.into(), eve.into(), ferdie.into()])
        .vested_accounts(vec![
            (dave.into(), 10, 100, 10_000 * UNITS), // 1 TOKEN unlocked per block
            (eve.into(), 10, 100, 10_000 * UNITS),
            (ferdie.into(), 10, 100, 10_000 * UNITS),
        ])
        .root(alice.into())
        .build()
        .execute_with(|| {
            let acc_data = System::account(dave.to_account_id()).data;
            // Free balance of vested accounts is still 100_000 TOKENS
            assert_eq!(acc_data.free, 100_000 * UNITS);
            // Locked balance is 90_000 TOKENS
            assert_eq!(acc_data.frozen, 90_000 * UNITS);

            // Locked  funds can't be reserved to pay for gas and/or value
            // Transaction should be invalidated when attempting to `reserve` currency:
            // - the required free balance is 10 * UNITS on gas + 10 * UNITS for `value`
            //   whereas the account only has 10 * UNITS unlocked
            assert_noop!(
                Gear::upload_program(
                    RuntimeOrigin::signed(dave.to_account_id()),
                    code.clone(),
                    b"salt".to_vec(),
                    vec![],
                    10_000_000_000,
                    10_000 * UNITS,
                    false,
                ),
                pallet_gear_bank::Error::<Runtime>::InsufficientBalance
            );

            // TODO: delete lines below (issue #3081).
            core::mem::drop(Balances::deposit_creating(
                &alice.to_account_id(),
                10_000 * UNITS,
            ));

            // Locked funds can't be transferred to a program as a message `value`
            assert_ok!(Gear::upload_program(
                RuntimeOrigin::signed(alice.to_account_id()),
                code,
                b"salt".to_vec(),
                vec![],
                10_000_000_000,
                0,
                false,
            ));
            let program_id = get_last_program_id();

            // Finalize program initialization
            run_to_block(2);

            // Try to send message to a program with value that exceeds the account free balance
            assert_noop!(
                Gear::send_message(
                    RuntimeOrigin::signed(dave.to_account_id()),
                    program_id.into(),
                    vec![],
                    10_000_000_000,
                    11_000 * UNITS,
                    false,
                ),
                pallet_gear_bank::Error::<Runtime>::InsufficientBalance
            );
        });
}

#[test]
fn treasury_surplus_is_not_burned() {
    init_logger();

    let alice = AccountKeyring::Alice;
    let bob = AccountKeyring::Bob;
    let charlie = AccountKeyring::Charlie;
    let dave = AccountKeyring::Dave;

    let treasury_id = Treasury::account_id();

    ExtBuilder::default()
        .initial_authorities(vec![
            (
                alice.into(),
                charlie.into(),
                alice.public(),
                ed25519::Pair::from_string("//Alice", None)
                    .unwrap()
                    .public(),
                alice.public(),
                alice.public(),
            ),
            (
                bob.into(),
                dave.into(),
                bob.public(),
                ed25519::Pair::from_string("//Bob", None).unwrap().public(),
                bob.public(),
                bob.public(),
            ),
        ])
        .stash(STASH)
        .endowment(ENDOWMENT)
        .endowed_accounts(vec![charlie.into(), dave.into()])
        .root(alice.into())
        .build()
        .execute_with(|| {
            // Treasury pot is empty in the beginning
            assert_eq!(Treasury::pot(), 0);

            let initial_total_issuance = Balances::total_issuance();

            // Top up treasury balance
            assert_ok!(Balances::transfer_allow_death(
                RuntimeOrigin::signed(charlie.to_account_id()),
                sp_runtime::MultiAddress::Id(treasury_id.clone()),
                1_000 * UNITS,
            ));
            assert_eq!(Treasury::pot(), 1_000 * UNITS);

            System::reset_events();

            // Run chain for a day so that `Treasury::spend_funds()` is triggered
            run_to_block(DAYS);

            // Check that the `Treasury::spend_funds()` has, indeed, taken place
            let (budget_remaining, burnt_funds, rollover_balance) = get_treasury_events();
            // Treasury remaining budget value upon entry in `spend_funds()` function
            assert_eq!(budget_remaining, 1_000 * UNITS);
            // Actually burnt funds
            assert_eq!(burnt_funds, 0);
            // Remaining balance being rolled over to the next period
            assert_eq!(rollover_balance, 1_000 * UNITS);

            // Treasury had a surplus, but none of it was burned
            assert_eq!(Treasury::pot(), 1_000 * UNITS);

            // The total issuance persisted
            assert_eq!(Balances::total_issuance(), initial_total_issuance);

            // Run chain until another `Treasury::spend_funds()` invocation
            run_to_block(2 * DAYS);

            // Treasury still has a surplus, but nothing is burned
            assert_eq!(Treasury::pot(), 1_000 * UNITS);

            assert_eq!(Balances::total_issuance(), initial_total_issuance);
        });
}

#[test]
fn dust_ends_up_in_offset_pool() {
    init_logger();

    let alice = AccountKeyring::Alice;
    let bob = AccountKeyring::Bob;
    let charlie = AccountKeyring::Charlie;
    let dave = AccountKeyring::Dave;
    let ferdie = AccountKeyring::Ferdie;

    let offset_pool_id = StakingRewards::account_id();

    ExtBuilder::default()
        .initial_authorities(vec![
            (
                alice.into(),
                charlie.into(),
                alice.public(),
                ed25519::Pair::from_string("//Alice", None)
                    .unwrap()
                    .public(),
                alice.public(),
                alice.public(),
            ),
            (
                bob.into(),
                dave.into(),
                bob.public(),
                ed25519::Pair::from_string("//Bob", None).unwrap().public(),
                bob.public(),
                bob.public(),
            ),
        ])
        .stash(STASH)
        .endowment(ENDOWMENT)
        .endowed_accounts(vec![charlie.into(), dave.into(), offset_pool_id.clone()])
        .root(alice.into())
        .build()
        .execute_with(|| {
            let initial_pool_balance = Balances::free_balance(&offset_pool_id);
            assert_eq!(initial_pool_balance, ENDOWMENT);

            let initial_total_issuance = Balances::total_issuance();

            // Sending ED to `ferdie` to create the account in storage
            assert_ok!(Balances::transfer_allow_death(
                RuntimeOrigin::signed(charlie.to_account_id()),
                sp_runtime::MultiAddress::Id(ferdie.to_account_id()),
                EXISTENTIAL_DEPOSIT,
            ));
            // `ferdie`'s balance is now ED
            assert_eq!(
                Balances::free_balance(ferdie.to_account_id()),
                EXISTENTIAL_DEPOSIT
            );

            // Sending ED / 2 out of `ferdie` creates dust
            assert_ok!(Balances::transfer_allow_death(
                RuntimeOrigin::signed(ferdie.to_account_id()),
                sp_runtime::MultiAddress::Id(dave.to_account_id()),
                EXISTENTIAL_DEPOSIT / 2,
            ));
            // `ferdie`'s balance is now 0
            assert_eq!(Balances::free_balance(ferdie.to_account_id()), 0);
            // Dust has been accumulated in the offset pool account
            assert_eq!(
                Balances::free_balance(&offset_pool_id),
                initial_pool_balance + EXISTENTIAL_DEPOSIT / 2
            );
            // The `total_issuance` has persisted
            assert_eq!(Balances::total_issuance(), initial_total_issuance);
        });
}

// Setting lock on an account prevents the account from being dusted
#[test]
fn dusting_prevented_by_lock() {
    init_logger();

    let alice = AccountKeyring::Alice;
    let bob = AccountKeyring::Bob;
    let charlie = AccountKeyring::Charlie;
    let dave = AccountKeyring::Dave;
    let ferdie = AccountKeyring::Ferdie;

    let offset_pool_id = StakingRewards::account_id();

    ExtBuilder::default()
        .initial_authorities(vec![
            (
                alice.into(),
                charlie.into(),
                alice.public(),
                ed25519::Pair::from_string("//Alice", None)
                    .unwrap()
                    .public(),
                alice.public(),
                alice.public(),
            ),
            (
                bob.into(),
                dave.into(),
                bob.public(),
                ed25519::Pair::from_string("//Bob", None).unwrap().public(),
                bob.public(),
                bob.public(),
            ),
        ])
        .stash(STASH)
        .endowment(ENDOWMENT)
        .endowed_accounts(vec![charlie.into(), dave.into(), offset_pool_id.clone()])
        .root(alice.into())
        .build()
        .execute_with(|| {
            let value = 1_000 * UNITS;

            // Sending ED + `value` to `ferdie` to create the account in storage
            assert_ok!(Balances::transfer_allow_death(
                RuntimeOrigin::signed(charlie.to_account_id()),
                sp_runtime::MultiAddress::Id(ferdie.to_account_id()),
                EXISTENTIAL_DEPOSIT + value,
            ));
            // `ferdie`'s balance is now ED + `value`
            assert_eq!(
                Balances::free_balance(ferdie.to_account_id()),
                EXISTENTIAL_DEPOSIT + value
            );

            // Sending out some value to create dust
            assert_ok!(Balances::transfer_allow_death(
                RuntimeOrigin::signed(ferdie.to_account_id()),
                sp_runtime::MultiAddress::Id(dave.to_account_id()),
                value + 1,
            ));
            // `ferdie`'s balance is now 0
            assert_eq!(Balances::free_balance(ferdie.to_account_id()), 0);

            // Second round
            assert_ok!(Balances::transfer_allow_death(
                RuntimeOrigin::signed(charlie.to_account_id()),
                sp_runtime::MultiAddress::Id(ferdie.to_account_id()),
                EXISTENTIAL_DEPOSIT + value,
            ));
            // `ferdie`'s balance is now (again) ED + `value`
            assert_eq!(
                Balances::free_balance(ferdie.to_account_id()),
                EXISTENTIAL_DEPOSIT + value
            );

            // Setting lock on `ferdie`'s account
            Balances::set_lock(
                *b"testlock",
                &ferdie.into(),
                EXISTENTIAL_DEPOSIT,
                WithdrawReasons::all(),
            );

            // Sending out the same amount of value as before will now fail
            assert_noop!(
                Balances::transfer_allow_death(
                    RuntimeOrigin::signed(ferdie.to_account_id()),
                    sp_runtime::MultiAddress::Id(dave.to_account_id()),
                    value + 1,
                ),
                sp_runtime::TokenError::Frozen
            );

            // Sending value so that the frozen amount is not touched is ok
            assert_ok!(Balances::transfer_allow_death(
                RuntimeOrigin::signed(ferdie.to_account_id()),
                sp_runtime::MultiAddress::Id(dave.to_account_id()),
                value,
            ));

            // `ferdie`'s balance is still greater than 0: exactly ED
            assert_eq!(
                Balances::free_balance(ferdie.to_account_id()),
                EXISTENTIAL_DEPOSIT
            );
        });
}

#[test]
fn fungible_api_works() {
    init_logger();

    let alice = AccountKeyring::Alice;
    let bob = AccountKeyring::Bob;
    let charlie = AccountKeyring::Charlie;

    let offset_pool_id = StakingRewards::account_id();

    ExtBuilder::default()
        .initial_authorities(vec![
            (
                alice.into(),
                alice.into(),
                alice.public(),
                ed25519::Pair::from_string("//Alice", None)
                    .unwrap()
                    .public(),
                alice.public(),
                alice.public(),
            ),
            (
                bob.into(),
                bob.into(),
                bob.public(),
                ed25519::Pair::from_string("//Bob", None).unwrap().public(),
                bob.public(),
                bob.public(),
            ),
        ])
        .stash(STASH)
        .endowment(ENDOWMENT)
        .endowed_accounts(vec![charlie.into(), offset_pool_id.clone()])
        .root(alice.into())
        .build()
        .execute_with(|| {
            let ok_value = 10 * EXISTENTIAL_DEPOSIT;
            let low_value = EXISTENTIAL_DEPOSIT / 2;

            // Check overflow
            Balances::make_free_balance_be(&charlie.into(), u128::MAX);
            assert_eq!(
                <Balances as fungible::Inspect<AccountId>>::can_deposit(
                    &charlie.into(),
                    ok_value,
                    Provenance::Extant
                ),
                DepositConsequence::Overflow
            );

            // Check below minimum
            Balances::make_free_balance_be(&charlie.into(), 0);
            assert_eq!(
                <Balances as fungible::Inspect<AccountId>>::can_deposit(
                    &charlie.into(),
                    low_value,
                    Provenance::Extant
                ),
                DepositConsequence::BelowMinimum
            );

            // Ok case
            assert_ok!(
                <Balances as fungible::Inspect<AccountId>>::can_deposit(
                    &charlie.into(),
                    ok_value,
                    Provenance::Extant
                )
                .into_result()
            );

            // Trivial check of reducible balance
            Balances::make_free_balance_be(&charlie.into(), 5 * EXISTENTIAL_DEPOSIT);
            assert_eq!(
                <Balances as fungible::Inspect<AccountId>>::reducible_balance(
                    &charlie.into(),
                    Preservation::Preserve,
                    Fortitude::Polite
                ),
                4 * EXISTENTIAL_DEPOSIT
            );

            assert_eq!(
                <Balances as fungible::Inspect<AccountId>>::reducible_balance(
                    &charlie.into(),
                    Preservation::Expendable,
                    Fortitude::Polite
                ),
                5 * EXISTENTIAL_DEPOSIT
            );

            // Reducible balance with a lock
            <Balances as LockableCurrency<AccountId>>::set_lock(
                *b"testlock",
                &charlie.into(),
                2 * EXISTENTIAL_DEPOSIT,
                WithdrawReasons::all(),
            );
            // Two existential deposits are locked
            assert_eq!(
                <Balances as fungible::Inspect<AccountId>>::reducible_balance(
                    &charlie.into(),
                    Preservation::Expendable,
                    Fortitude::Polite
                ),
                3 * EXISTENTIAL_DEPOSIT
            );

            // Set the free balance to the amount below what is frozen, but greater than 0
            Balances::make_free_balance_be(&charlie.into(), EXISTENTIAL_DEPOSIT);
            assert_eq!(
                <Balances as fungible::Inspect<AccountId>>::reducible_balance(
                    &charlie.into(),
                    Preservation::Expendable,
                    Fortitude::Polite
                ),
                0
            );

            // Remove lock
            <Balances as LockableCurrency<AccountId>>::remove_lock(*b"testlock", &charlie.into());
            assert_eq!(
                <Balances as fungible::Inspect<AccountId>>::reducible_balance(
                    &charlie.into(),
                    Preservation::Expendable,
                    Fortitude::Polite
                ),
                EXISTENTIAL_DEPOSIT
            );
        });
}

#[test]
fn process_costs_are_same() {
    let vara_schedule = pallet_gear::Schedule::<super::Runtime>::default();
    let wasm_schedule = gear_core::gas_metering::Schedule::default();

    assert_eq!(vara_schedule.process_costs(), wasm_schedule.process_costs());
}

fn all_measured_instructions() -> Vec<Instruction> {
    use Instruction::*;

    let default_table_data = gear_wasm_instrument::BrTable {
        default: 0,
        targets: vec![],
    };

    // A set of instructions weights for which the Gear provides.
    // Instruction must not be removed (!), but can be added.
    vec![
        End,
        Unreachable,
        Return,
        Else,
        I32Const(0),
        I64Const(0),
        Block(BlockType::Empty),
        Loop(BlockType::Empty),
        Nop,
        Drop,
        I32Load(MemArg::zero()),
        I32Load8S(MemArg::zero()),
        I32Load8U(MemArg::zero()),
        I32Load16S(MemArg::zero()),
        I32Load16U(MemArg::zero()),
        I64Load(MemArg::zero()),
        I64Load8S(MemArg::zero()),
        I64Load8U(MemArg::zero()),
        I64Load16S(MemArg::zero()),
        I64Load16U(MemArg::zero()),
        I64Load32S(MemArg::zero()),
        I64Load32U(MemArg::zero()),
        I32Store(MemArg::zero()),
        I32Store8(MemArg::zero()),
        I32Store16(MemArg::zero()),
        I64Store(MemArg::zero()),
        I64Store8(MemArg::zero()),
        I64Store16(MemArg::zero()),
        I64Store32(MemArg::zero()),
        Select,
        If(BlockType::Empty),
        Br(0),
        BrIf(0),
        Call(0),
        LocalGet(0),
        LocalSet(0),
        LocalTee(0),
        GlobalGet(0),
        GlobalSet(0),
        MemorySize(0),
        CallIndirect(0),
        BrTable(default_table_data),
        I32Clz,
        I64Clz,
        I32Ctz,
        I64Ctz,
        I32Popcnt,
        I64Popcnt,
        I32Eqz,
        I64Eqz,
        I64ExtendI32S,
        I64ExtendI32U,
        I32WrapI64,
        I32Eq,
        I64Eq,
        I32Ne,
        I64Ne,
        I32LtS,
        I64LtS,
        I32LtU,
        I64LtU,
        I32GtS,
        I64GtS,
        I32GtU,
        I64GtU,
        I32LeS,
        I64LeS,
        I32LeU,
        I64LeU,
        I32GeS,
        I64GeS,
        I32GeU,
        I64GeU,
        I32Add,
        I64Add,
        I32Sub,
        I64Sub,
        I32Mul,
        I64Mul,
        I32DivS,
        I64DivS,
        I32DivU,
        I64DivU,
        I32RemS,
        I64RemS,
        I32RemU,
        I64RemU,
        I32And,
        I64And,
        I32Or,
        I64Or,
        I32Xor,
        I64Xor,
        I32Shl,
        I64Shl,
        I32ShrS,
        I64ShrS,
        I32ShrU,
        I64ShrU,
        I32Rotl,
        I64Rotl,
        I32Rotr,
        I64Rotr,
    ]
}

fn default_wasm_module() -> Module {
    let simple_wat = r#"
    (module
        (import "env" "memory" (memory 1))
        (export "handle" (func $handle))
        (export "init" (func $init))
        (func $handle)
        (func $init)
    )"#;
    Module::new(&wat::parse_str(simple_wat).expect("failed to parse module"))
        .expect("module instantiation failed")
}

// This test must never fail during local development/release.
//
// The instruction set in the test mustn't be changed. Test checks
// that instructions weights in runtime and in gear-core are the same
#[test]
fn schedules_are_same() {
    let vara_schedule = pallet_gear::Schedule::<super::Runtime>::default();
    let wasm_schedule = gear_core::gas_metering::Schedule::default();

    let schedule_rules = vara_schedule.rules(&default_wasm_module());
    let wasm_instrument_schedule_rules = wasm_schedule.rules(&default_wasm_module());

    all_measured_instructions().iter().for_each(|i| {
        assert_eq!(
            schedule_rules.instruction_cost(i),
            wasm_instrument_schedule_rules.instruction_cost(i)
        );
    });
}

// This test must never fail during local development/release.
//
// The instruction set in the test mustn't be changed. Test checks
// whether no instruction weight was removed from Rules, so backward
// compatibility is reached.
#[test]
fn instruction_backward_compatibility() {
    let vara_schedule = pallet_gear::Schedule::<super::Runtime>::default();

    let schedule_rules = vara_schedule.rules(&default_wasm_module());
    let custom_cost_rules = CustomConstantCostRules::default();

    all_measured_instructions().iter().for_each(|i| {
        assert!(custom_cost_rules.instruction_cost(i).is_some());
        assert!(schedule_rules.instruction_cost(i).is_some());
    });
}

#[test]
fn test_fees_and_tip_split() {
    init_logger();

    let alice = AccountKeyring::Alice;
    let charlie = AccountKeyring::Charlie;

    ExtBuilder::default()
        .initial_authorities(vec![(
            alice.into(),
            charlie.into(),
            alice.public(),
            ed25519::Pair::from_string("//Alice", None)
                .unwrap()
                .public(),
            alice.public(),
            alice.public(),
        )])
        .stash(STASH)
        .endowment(ENDOWMENT)
        .endowed_accounts(vec![])
        .root(alice.into())
        .build()
        .execute_with(|| {
            const FEE: Balance = 137;
            const TIP: Balance = 42;

            let fee = Balances::issue(FEE);
            let tip = Balances::issue(TIP);

            assert_eq!(
                Balances::free_balance(Treasury::account_id()),
                EXISTENTIAL_DEPOSIT
            );
            assert_eq!(Balances::free_balance(alice.to_account_id()), STASH);

            DealWithFees::on_unbalanceds(vec![fee, tip].into_iter());

            // Current setting.
            assert_eq!(TreasuryTxFeeShare::get(), Percent::one());

            // Author gets 100% of the tip and 0% of the fee
            assert_eq!(Balances::free_balance(alice.to_account_id()), STASH + TIP);

            // Treasury gets 100% of the fee and 0% of the tip
            assert_eq!(
                Balances::free_balance(Treasury::account_id()),
                EXISTENTIAL_DEPOSIT + FEE
            );
        });
}
