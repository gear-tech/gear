// Copyright (C) Gear Technologies Inc.
// SPDX-License-Identifier: GPL-3.0-or-later WITH Classpath-exception-2.0

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
use sp_core::{Pair, ecdsa, ed25519, sr25519};
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
    ecdsa::Public,
);

/// Deterministic ECDSA (BEEFY) public key for a `//<seed>` test account.
fn beefy_key(seed: &str) -> ecdsa::Public {
    ecdsa::Pair::from_string(&format!("//{seed}"), None)
        .expect("static seed is valid; qed")
        .public()
}

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
                    let (owner, keys) = session_validator(x);
                    (owner.clone(), owner, keys)
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
                beefy_key("Alice"),
            ),
            (
                bob.into(),
                dave.into(),
                bob.public(),
                ed25519::Pair::from_string("//Bob", None).unwrap().public(),
                bob.public(),
                bob.public(),
                beefy_key("Bob"),
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
                beefy_key("Alice"),
            ),
            (
                bob.into(),
                dave.into(),
                bob.public(),
                ed25519::Pair::from_string("//Bob", None).unwrap().public(),
                bob.public(),
                bob.public(),
                beefy_key("Bob"),
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
                beefy_key("Alice"),
            ),
            (
                bob.into(),
                dave.into(),
                bob.public(),
                ed25519::Pair::from_string("//Bob", None).unwrap().public(),
                bob.public(),
                bob.public(),
                beefy_key("Bob"),
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
                beefy_key("Alice"),
            ),
            (
                bob.into(),
                dave.into(),
                bob.public(),
                ed25519::Pair::from_string("//Bob", None).unwrap().public(),
                bob.public(),
                bob.public(),
                beefy_key("Bob"),
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
                beefy_key("Alice"),
            ),
            (
                bob.into(),
                bob.into(),
                bob.public(),
                ed25519::Pair::from_string("//Bob", None).unwrap().public(),
                bob.public(),
                bob.public(),
                beefy_key("Bob"),
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
            beefy_key("Alice"),
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

fn bridge_test_authority(account: AccountKeyring, seed: &str) -> ValidatorAccountId {
    let account_id = account.to_account_id();
    (
        account_id.clone(),
        account_id,
        account.public(),
        ed25519::Pair::from_string(&format!("//{seed}"), None)
            .expect("static seed is valid")
            .public(),
        account.public(),
        account.public(),
        beefy_key(seed),
    )
}

fn session_validator(authority: &ValidatorAccountId) -> (AccountId, SessionKeys) {
    (
        authority.0.clone(),
        SessionKeys {
            babe: authority.2.into(),
            grandpa: authority.3.into(),
            im_online: authority.4.into(),
            authority_discovery: authority.5.into(),
            beefy: authority.6.into(),
        },
    )
}

fn read_mmr_leaf(ext: &mut sp_io::TestExternalities, key: Vec<u8>) -> crate::mmr::Leaf {
    type Node = pallet_mmr::primitives::DataOrHash<Keccak256, crate::mmr::Leaf>;

    ext.persist_offchain_overlay();
    match Node::decode(
        &mut ext
            .offchain_db()
            .get(&key)
            .expect("MMR leaf was written")
            .as_slice(),
    )
    .expect("MMR node decodes")
    {
        Node::Data(leaf) => leaf,
        Node::Hash(_) => panic!("MMR leaf position must contain leaf data"),
    }
}

#[test]
fn bridge_finalization_at_n_is_committed_at_n_plus_one() {
    use frame_support::traits::{OnFinalize, OnInitialize, OneSessionHandler};
    use sp_consensus_beefy::mmr::BeefyDataProvider;
    use sp_core::offchain::{OffchainDbExt, testing::TestOffchainExt};
    use sp_runtime::traits::Hash;

    let alice = bridge_test_authority(AccountKeyring::Alice, "Alice");
    let mut ext = ExtBuilder::default()
        .initial_authorities(vec![alice.clone()])
        .stash(STASH)
        .build();
    let parent_hash = sp_core::H256::repeat_byte(0x55);

    let (first_key, first_extra, key, expected_extra, committed_root) = ext.execute_with(|| {
        frame_system::BlockHash::<Runtime>::insert(0, sp_core::H256::repeat_byte(0x99));
        System::initialize(&1, &System::block_hash(0), &Default::default());
        let uninitialized_extra = bridge_leaf::VaraBridgeProvider::extra_data();
        let alice = session_validator(&alice);
        <GearEthBridge as OneSessionHandler<AccountId>>::on_new_session(
            true,
            [(&alice.0, alice.1.grandpa.clone())].into_iter(),
            [(&alice.0, alice.1.grandpa.clone())].into_iter(),
        );
        let first_extra = bridge_leaf::VaraBridgeProvider::extra_data();
        assert_ne!(first_extra, uninitialized_extra);
        assert_eq!(
            GearEthBridge::bridge_snapshot(),
            Some((0, sp_core::H256::zero()))
        );
        Mmr::on_initialize(1);
        let first_key = (
            <Runtime as pallet_mmr::Config>::INDEXING_PREFIX,
            0_u64,
            System::parent_hash(),
        )
            .encode();
        assert_ok!(GearEthBridge::unpause(RuntimeOrigin::root()));
        assert_ok!(GearEthBridge::send_eth_message(
            RuntimeOrigin::signed(alice.0),
            sp_core::H160::repeat_byte(1),
            vec![1, 2, 3],
        ));
        GearEthBridge::on_finalize(1);
        assert_ok!(GearEthBridge::pause(RuntimeOrigin::root()));
        assert_ok!(GearEthBridge::bind_destination(
            RuntimeOrigin::root(),
            sp_core::H256::repeat_byte(2),
            sp_core::H160::repeat_byte(3),
        ));
        assert_ok!(Timestamp::set(RuntimeOrigin::none(), SLOT_DURATION));
        Timestamp::on_finalize(1);
        let (queue_id, queue_root) = GearEthBridge::bridge_snapshot().unwrap();
        let expected_extra = Keccak256::hash(
            &(
                2u8,
                *b"vara",
                GearEthBridge::bridge_domain(),
                SLOT_DURATION,
                1u8,
                queue_id,
                queue_root,
            )
                .encode(),
        )
        .0;
        System::initialize(&2, &parent_hash, &Default::default());
        Mmr::on_initialize(2);
        let key = (
            <Runtime as pallet_mmr::Config>::INDEXING_PREFIX,
            1_u64,
            System::parent_hash(),
        )
            .encode();
        let committed_root = Mmr::mmr_root();
        assert!(System::digest().logs.iter().any(|log| log
            == &sp_runtime::DigestItem::Consensus(
                sp_consensus_beefy::BEEFY_ENGINE_ID,
                sp_consensus_beefy::ConsensusLog::<BeefyId>::MmrRoot(committed_root).encode(),
            )));
        (first_key, first_extra, key, expected_extra, committed_root)
    });

    let first_leaf = read_mmr_leaf(&mut ext, first_key);
    assert_eq!(first_leaf.parent_number_and_hash.0, 0);
    assert_eq!(first_leaf.leaf_extra, first_extra);
    assert_eq!(first_leaf.encode().len(), 113);
    assert_eq!(first_leaf.encode()[0], 0);
    let leaf = read_mmr_leaf(&mut ext, key);
    assert_eq!(leaf.leaf_extra, expected_extra);
    assert_eq!(leaf.parent_number_and_hash, (1, parent_hash));
    assert_ne!(leaf.leaf_extra, first_leaf.leaf_extra);

    ext.execute_with(|| {
        System::initialize(&3, &sp_core::H256::repeat_byte(0x77), &Default::default());
        Mmr::on_initialize(3);
        assert_ne!(Mmr::mmr_root(), committed_root);
    });
    ext.persist_offchain_overlay();
    let (offchain, _) = TestOffchainExt::with_offchain_db(ext.offchain_db());
    ext.register_extension(OffchainDbExt::new(offchain));
    ext.execute_with(|| {
        let output = crate::api::dispatch("MmrApi_generate_proof", &(vec![2u32], Some(2u32)).encode()).unwrap();
        type ProofResult = Result<(Vec<mmr::EncodableOpaqueLeaf>, sp_mmr_primitives::LeafProof<sp_core::H256>), sp_mmr_primitives::Error>;
        let (leaves, proof) = <ProofResult as parity_scale_codec::DecodeAll>::decode_all(&mut output.as_slice()).unwrap().unwrap();
        assert_eq!(leaves, vec![mmr::EncodableOpaqueLeaf::from_leaf(&leaf)]);
        for root in [committed_root, Mmr::mmr_root()] {
            let output = crate::api::dispatch("MmrApi_verify_proof_stateless", &(root, &leaves, &proof).encode()).unwrap();
            let result = <Result<(), sp_mmr_primitives::Error> as parity_scale_codec::DecodeAll>::decode_all(&mut output.as_slice()).unwrap();
            assert_eq!(result.is_ok(), root == committed_root);
        }
        let mut corrupted = leaf;
        corrupted.leaf_extra[0] ^= 1;
        let output = crate::api::dispatch("MmrApi_verify_proof_stateless", &(committed_root, vec![mmr::EncodableOpaqueLeaf::from_leaf(&corrupted)], proof).encode()).unwrap();
        assert!(<Result<(), sp_mmr_primitives::Error> as parity_scale_codec::DecodeAll>::decode_all(&mut output.as_slice()).unwrap().is_err());
    });
}

#[test]
fn session_boundary_leaf_uses_new_authorities_and_preclear_bridge_root() {
    use frame_support::traits::{OnFinalize, OnInitialize, OneSessionHandler};
    use pallet_session::SessionHandler;
    use sp_consensus_beefy::mmr::BeefyDataProvider;
    use sp_runtime::traits::Hash;

    let alice = bridge_test_authority(AccountKeyring::Alice, "Alice");
    let bob = bridge_test_authority(AccountKeyring::Bob, "Bob");
    let charlie = bridge_test_authority(AccountKeyring::Charlie, "Charlie");
    let mut ext = ExtBuilder::default()
        .initial_authorities(vec![alice.clone()])
        .stash(STASH)
        .build();

    let genesis = sp_core::H256::repeat_byte(0x99);
    let timestamp1 = SLOT_DURATION;
    let timestamp2 = 2 * SLOT_DURATION;
    let (key, expected_extra, expected_authorities_root, old_id, first_root, hash) = ext
        .execute_with(|| {
            frame_system::BlockHash::<Runtime>::insert(0, genesis);
            let alice = session_validator(&alice);
            let bob = session_validator(&bob);
            let charlie = session_validator(&charlie);
            let beefy_address =
                <pallet_beefy_mmr::BeefyEcdsaToEthereum as sp_runtime::traits::Convert<
                    _,
                    Vec<u8>,
                >>::convert(charlie.1.beefy.clone());
            let expected_authorities_root =
                <<Runtime as pallet_mmr::Config>::Hashing as sp_runtime::traits::Hash>::hash(
                    &beefy_address,
                );

            <GearEthBridge as OneSessionHandler<AccountId>>::on_new_session(
                true,
                [(&alice.0, alice.1.grandpa.clone())].into_iter(),
                [(&alice.0, alice.1.grandpa.clone())].into_iter(),
            );
            assert_ok!(GearEthBridge::unpause(RuntimeOrigin::root()));
            assert_ok!(GearEthBridge::send_eth_message(
                RuntimeOrigin::signed(alice.0.clone()),
                sp_core::H160::repeat_byte(2),
                vec![4, 5, 6],
            ));
            let first_root = last_bridge_message().1;
            assert_ok!(GearEthBridge::send_eth_message(
                RuntimeOrigin::signed(alice.0.clone()),
                sp_core::H160::repeat_byte(3),
                vec![7, 8, 9],
            ));
            let (_, hash) = last_bridge_message();
            GearEthBridge::on_finalize(1);
            assert_ok!(Timestamp::set(RuntimeOrigin::none(), timestamp1));
            Timestamp::on_finalize(1);
            Babe::on_finalize(1);
            let (old_id, root) = GearEthBridge::bridge_snapshot().unwrap();
            assert_ne!(root, sp_core::H256::zero());
            let expected_extra = bridge_leaf::VaraBridgeProvider::extra_data();
            assert_eq!(
                expected_extra,
                Keccak256::hash(
                    &[
                        &[2][..],
                        b"vara",
                        GearEthBridge::bridge_domain().as_bytes(),
                        &timestamp1.to_le_bytes(),
                        &[1],
                        &old_id.to_le_bytes(),
                        root.as_bytes(),
                    ]
                    .concat()
                )
                .0
            );

            initialize_block(2);
            // The manual session transition needs BABE's current-block pre-digest first.
            let _ = <Babe as pallet_session::ShouldEndSession<BlockNumber>>::should_end_session(2);
            <VaraSessionHandler as SessionHandler<AccountId>>::on_new_session(
                true,
                std::slice::from_ref(&bob),
                &[charlie],
            );
            <AllPalletsWithSystem as OnInitialize<BlockNumberFor<Runtime>>>::on_initialize(2);
            let key = (
                <Runtime as pallet_mmr::Config>::INDEXING_PREFIX,
                0_u64,
                System::parent_hash(),
            )
                .encode();
            assert_eq!(Timestamp::get(), timestamp1);
            (
                key,
                expected_extra,
                expected_authorities_root,
                old_id,
                first_root,
                hash,
            )
        });

    let leaf = read_mmr_leaf(&mut ext, key.clone());
    assert_eq!(leaf.leaf_extra, expected_extra);
    assert_eq!(leaf.beefy_next_authority_set.id, 2);
    assert_eq!(leaf.beefy_next_authority_set.len, 1);
    assert_eq!(
        leaf.beefy_next_authority_set.keyset_commitment,
        expected_authorities_root
    );
    assert_eq!(leaf.parent_number_and_hash.0, 1);
    assert_eq!(leaf.encode().len(), 113);
    assert_eq!(leaf.encode()[0], 0);

    let first_key = key;
    let (key, final_extra) = ext.execute_with(|| {
        assert_eq!(System::block_hash(0), genesis);
        assert_ok!(Timestamp::set(RuntimeOrigin::none(), timestamp2));
        assert_ne!(
            bridge_leaf::VaraBridgeProvider::extra_data(),
            leaf.leaf_extra
        );
        assert_noop!(
            GearEthBridge::send_eth_message(
                RuntimeOrigin::signed(alice.0.clone()),
                sp_core::H160::repeat_byte(3),
                vec![7, 8, 9],
            ),
            pallet_gear_eth_bridge::Error::<Runtime>::BridgeCleanupRequired
        );
        GearEthBridge::on_finalize(2);
        let proof = GearEthBridge::merkle_proof(hash).expect("final message has a proof");
        assert_eq!(proof.leaf, hash);
        assert_eq!(proof.leaf_index, 1);
        assert_eq!(proof.number_of_leaves, 2);
        assert_eq!(proof.proof, vec![first_root]);
        assert_eq!(
            proof.root,
            Keccak256::hash(&[first_root.as_bytes(), hash.as_bytes()].concat())
        );
        assert_eq!(GearEthBridge::bridge_snapshot(), Some((old_id, proof.root)));
        let extra = bridge_leaf::VaraBridgeProvider::extra_data();
        Timestamp::on_finalize(2);
        Babe::on_finalize(2);
        initialize_block(3);
        <AllPalletsWithSystem as OnInitialize<BlockNumberFor<Runtime>>>::on_initialize(3);
        let key = (
            <Runtime as pallet_mmr::Config>::INDEXING_PREFIX,
            1_u64,
            System::parent_hash(),
        )
            .encode();
        assert_eq!(Timestamp::get(), timestamp2);
        assert_eq!(
            GearEthBridge::bridge_snapshot(),
            Some((old_id + 1, sp_core::H256::zero()))
        );
        assert!(GearEthBridge::merkle_proof(hash).is_none());
        (key, extra)
    });
    assert_eq!(read_mmr_leaf(&mut ext, first_key).encode(), leaf.encode());
    let final_leaf = read_mmr_leaf(&mut ext, key);
    assert_eq!(final_leaf.parent_number_and_hash.0, 2);
    assert_eq!(final_leaf.encode().len(), 113);
    assert_eq!(final_leaf.encode()[0], 0);
    assert_eq!(final_leaf.leaf_extra, final_extra);
    assert_ne!(final_leaf.leaf_extra, leaf.leaf_extra);
    assert_eq!(
        final_leaf.beefy_next_authority_set,
        leaf.beefy_next_authority_set
    );
}

#[test]
fn inactive_beefy_still_allows_unsigned_mmr_root_digests() {
    use frame_support::traits::OnInitialize;
    use sp_consensus_beefy::{BEEFY_ENGINE_ID, ConsensusLog};

    let alice = bridge_test_authority(AccountKeyring::Alice, "Alice");
    ExtBuilder::default()
        .initial_authorities(vec![alice])
        .stash(STASH)
        .build()
        .execute_with(|| {
            assert!(pallet_beefy::GenesisBlock::<Runtime>::get().is_none());

            initialize_block(2);
            Mmr::on_initialize(2);

            assert!(System::digest().logs.iter().any(|digest| {
                matches!(
                    digest,
                    DigestItem::Consensus(engine, data)
                        if *engine == BEEFY_ENGINE_ID
                            && matches!(
                                ConsensusLog::<BeefyId>::decode(&mut data.as_slice()),
                                Ok(ConsensusLog::MmrRoot(_))
                            )
                )
            }));
            assert!(!System::digest().logs.iter().any(
                |digest| matches!(digest, DigestItem::Seal(engine, _) if *engine == BEEFY_ENGINE_ID)
            ));
        });
}

#[test]
fn beefy_only_session_changes_preserve_the_original_bridge_queue() {
    use frame_support::traits::OneSessionHandler;
    use pallet_session::SessionHandler;

    let alice = bridge_test_authority(AccountKeyring::Alice, "Alice");
    ExtBuilder::default()
        .initial_authorities(vec![alice.clone()])
        .stash(STASH)
        .build()
        .execute_with(|| {
            let alice = session_validator(&alice);
            <grandpa_keys_handler::GrandpaAndGearEthBridge as OneSessionHandler<AccountId>>::on_new_session(
                true,
                [(&alice.0, alice.1.grandpa.clone())].into_iter(),
                [(&alice.0, alice.1.grandpa.clone())].into_iter(),
            );
            assert_ok!(GearEthBridge::unpause(RuntimeOrigin::root()));
            assert_ok!(GearEthBridge::send_eth_message(
                RuntimeOrigin::signed(alice.0.clone()),
                sp_core::H160::repeat_byte(4),
                vec![10, 11, 12],
            ));
            GearEthBridge::on_finalize(1);
            Grandpa::on_finalize(1);
            let original_grandpa_set_id = Grandpa::current_set_id();
            let original_grandpa_authorities = Grandpa::grandpa_authorities();
            let original = GearEthBridge::bridge_snapshot().expect("bridge initialized");
            assert_ne!(original.1, sp_core::H256::zero());
            let mut rotated = alice;
            rotated.1.beefy = beefy_key("Bob").into();

            initialize_block(2);
            let _ = <Babe as pallet_session::ShouldEndSession<BlockNumber>>::should_end_session(2);
            <VaraSessionHandler as SessionHandler<AccountId>>::on_new_session(
                true,
                std::slice::from_ref(&rotated),
                std::slice::from_ref(&rotated),
            );
            Grandpa::on_finalize(2);
            assert_eq!(Grandpa::current_set_id(), original_grandpa_set_id + 1);
            assert_eq!(Grandpa::grandpa_authorities(), original_grandpa_authorities);
            GearEthBridge::on_initialize(2);
            GearEthBridge::on_initialize(3);
            assert_eq!(GearEthBridge::bridge_snapshot(), Some(original));
            assert_eq!(Beefy::validator_set().unwrap().validators(), &vec![rotated.1.beefy]);

            initialize_block(4);
            Mmr::on_initialize(4);
            let mmr_root = pallet_mmr::RootHash::<Runtime>::get();
            let leaves = pallet_mmr::NumberOfLeaves::<Runtime>::get();
            assert_eq!(pallet_beefy::GenesisBlock::<Runtime>::get(), None);
            assert_noop!(
                Beefy::set_new_genesis(RuntimeOrigin::signed(rotated.0), 1),
                sp_runtime::DispatchError::BadOrigin,
            );

            // A raw domain write alone must not admit unsigned or unactivated keys.
            assert_noop!(
                Utility::batch_all(
                    RuntimeOrigin::root(),
                    vec![RuntimeCall::Beefy(pallet_beefy::Call::set_new_genesis { delay_in_blocks: 1 })],
                ).map_err(|error| error.error),
                sp_runtime::DispatchError::BadOrigin,
            );
            assert_eq!(pallet_beefy::GenesisBlock::<Runtime>::get(), None);
            assert_eq!(GearEthBridge::bridge_snapshot(), Some(original));
            assert_eq!(pallet_mmr::RootHash::<Runtime>::get(), mmr_root);
            assert_eq!(pallet_mmr::NumberOfLeaves::<Runtime>::get(), leaves);
        });
}

fn last_bridge_message() -> (pallet_gear_eth_bridge::EthMessage, sp_core::H256) {
    System::events()
        .into_iter()
        .rev()
        .find_map(|record| match record.event {
            RuntimeEvent::GearEthBridge(pallet_gear_eth_bridge::Event::MessageQueued {
                message,
                hash,
            }) => Some((message, hash)),
            _ => None,
        })
        .expect("message was queued")
}

#[test]
fn scheduled_governance_send_at_session_clear_is_rejected_without_losing_nonce() {
    use frame_support::traits::{
        OneSessionHandler, StorePreimage,
        schedule::{DispatchTime, v3::Anon},
    };
    use pallet_session::SessionHandler;
    use sp_runtime::traits::Hash;

    let alice = bridge_test_authority(AccountKeyring::Alice, "Alice");
    let bob = bridge_test_authority(AccountKeyring::Bob, "Bob");
    ExtBuilder::default()
        .initial_authorities(vec![alice.clone()])
        .stash(STASH)
        .build()
        .execute_with(|| {
            let alice = session_validator(&alice);
            let bob = session_validator(&bob);
            <GearEthBridge as OneSessionHandler<AccountId>>::on_new_session(
                true,
                [(&alice.0, alice.1.grandpa.clone())].into_iter(),
                [(&alice.0, alice.1.grandpa.clone())].into_iter(),
            );
            let governance = [
                GearEthBridgeAdminAccount::get(),
                GearEthBridgePauserAccount::get(),
            ];
            assert_ok!(GearEthBridge::send_eth_message(
                RuntimeOrigin::signed(governance[0].clone()),
                sp_core::H160::repeat_byte(1),
                vec![1],
            ));
            assert_eq!(last_bridge_message().0.nonce(), sp_core::U256::zero());
            GearEthBridge::on_finalize(1);
            let original = GearEthBridge::bridge_snapshot().unwrap();
            assert_ne!(original.1, sp_core::H256::zero());
            assert_ok!(Timestamp::set(RuntimeOrigin::none(), SLOT_DURATION));
            Timestamp::on_finalize(1);
            Babe::on_finalize(1);

            let tasks = governance
                .iter()
                .map(|account| {
                    <Scheduler as Anon<BlockNumber, RuntimeCall, OriginCaller>>::schedule(
                        DispatchTime::At(3),
                        None,
                        0,
                        frame_system::RawOrigin::Signed(account.clone()).into(),
                        Preimage::bound(RuntimeCall::GearEthBridge(
                            pallet_gear_eth_bridge::Call::send_eth_message {
                                destination: sp_core::H160::repeat_byte(2),
                                payload: vec![2],
                            },
                        ))
                        .unwrap(),
                    )
                    .unwrap()
                })
                .collect::<Vec<_>>();
            initialize_block(2);
            let _ = <Babe as pallet_session::ShouldEndSession<BlockNumber>>::should_end_session(2);
            <VaraSessionHandler as SessionHandler<AccountId>>::on_new_session(
                true,
                std::slice::from_ref(&bob),
                std::slice::from_ref(&bob),
            );
            <AllPalletsWithSystem as OnInitialize<BlockNumberFor<Runtime>>>::on_initialize(2);
            assert_eq!(GearEthBridge::bridge_snapshot(), Some(original));
            GearEthBridge::on_finalize(2);
            assert_ok!(Timestamp::set(RuntimeOrigin::none(), 2 * SLOT_DURATION));
            Timestamp::on_finalize(2);
            Babe::on_finalize(2);

            initialize_block(3);
            System::reset_events();
            // Runtime ordering runs Scheduler before the bridge's delayed clear.
            <AllPalletsWithSystem as OnInitialize<BlockNumberFor<Runtime>>>::on_initialize(3);
            let dispatches = System::events()
                .into_iter()
                .filter_map(|record| match record.event {
                    RuntimeEvent::Scheduler(pallet_scheduler::Event::Dispatched {
                        task,
                        result,
                        ..
                    }) => Some((task, result)),
                    _ => None,
                })
                .collect::<Vec<_>>();
            assert_eq!(
                dispatches,
                tasks
                    .into_iter()
                    .map(|task| (
                        task,
                        Err(pallet_gear_eth_bridge::Error::<Runtime>::BridgeCleanupRequired.into()),
                    ))
                    .collect::<Vec<_>>()
            );
            assert!(!System::events().iter().any(|record| matches!(
                record.event,
                RuntimeEvent::GearEthBridge(pallet_gear_eth_bridge::Event::MessageQueued { .. })
            )));
            assert_eq!(
                GearEthBridge::bridge_snapshot(),
                Some((original.0 + 1, sp_core::H256::zero()))
            );

            let hashes = governance
                .into_iter()
                .enumerate()
                .map(|(index, account)| {
                    assert_ok!(GearEthBridge::send_eth_message(
                        RuntimeOrigin::signed(account),
                        sp_core::H160::repeat_byte(3),
                        vec![3],
                    ));
                    let (message, hash) = last_bridge_message();
                    assert_eq!(message.nonce(), sp_core::U256::from(index + 1));
                    hash
                })
                .collect::<Vec<_>>();
            GearEthBridge::on_finalize(3);
            let root = Keccak256::hash(&[hashes[0].as_bytes(), hashes[1].as_bytes()].concat());
            assert_eq!(
                GearEthBridge::bridge_snapshot(),
                Some((original.0 + 1, root))
            );
            for (index, hash) in hashes.iter().enumerate() {
                let proof = GearEthBridge::merkle_proof(*hash).unwrap();
                assert_eq!(proof.leaf, *hash);
                assert_eq!(proof.leaf_index, index as u64);
                assert_eq!(proof.number_of_leaves, 2);
                assert_eq!(proof.proof, vec![hashes[1 - index]]);
                assert_eq!(proof.root, root);
            }
        });
}

#[test]
fn governance_append_requires_newer_real_grandpa_proof_before_overflow_reset() {
    use frame_support::traits::OneSessionHandler;
    use sp_runtime::traits::Header as _;

    fn finality_proof(header: &Header) -> Vec<u8> {
        let pair = ed25519::Pair::from_string("//Alice", None).unwrap();
        let round = 1;
        let precommit = sp_consensus_grandpa::Precommit::<Header> {
            target_hash: header.hash(),
            target_number: *header.number(),
        };
        let payload = sp_consensus_grandpa::localized_payload(
            round,
            Grandpa::current_set_id(),
            &sp_consensus_grandpa::Message::<Header>::Precommit(precommit.clone()),
        );
        // SCALE's signed-precommit tuple avoids adding a finality-grandpa dependency.
        let signed = (
            precommit,
            pair.sign(&payload),
            GrandpaId::from(pair.public()),
        )
            .encode();
        let justification = sp_consensus_grandpa::GrandpaJustification::<Header> {
            round,
            commit: sp_consensus_grandpa::Commit::<Header> {
                target_hash: header.hash(),
                target_number: *header.number(),
                precommits: vec![Decode::decode(&mut signed.as_slice()).unwrap()],
            },
            votes_ancestries: vec![],
        };
        // FinalityProof's public wire format: block, encoded justification, unknown headers.
        (header.hash(), justification.encode(), Vec::<Header>::new()).encode()
    }

    let alice = bridge_test_authority(AccountKeyring::Alice, "Alice");
    ExtBuilder::default()
        .initial_authorities(vec![alice.clone()])
        .stash(STASH)
        .build()
        .execute_with(|| {
            let alice = session_validator(&alice);
            <GearEthBridge as OneSessionHandler<AccountId>>::on_new_session(
                true,
                [(&alice.0, alice.1.grandpa.clone())].into_iter(),
                [(&alice.0, alice.1.grandpa.clone())].into_iter(),
            );
            assert_eq!(
                Grandpa::grandpa_authorities(),
                vec![(alice.1.grandpa.clone(), 1)]
            );
            assert_ok!(GearEthBridge::unpause(RuntimeOrigin::root()));
            let admin = GearEthBridgeAdminAccount::get();
            let capacity = <Runtime as pallet_gear_eth_bridge::Config>::QueueCapacity::get();
            for _ in 0..capacity {
                assert_ok!(GearEthBridge::send_eth_message(
                    RuntimeOrigin::signed(admin.clone()),
                    sp_core::H160::repeat_byte(4),
                    vec![4],
                ));
            }
            GearEthBridge::on_finalize(1);
            let original = GearEthBridge::bridge_snapshot().unwrap();
            assert_ok!(Timestamp::set(RuntimeOrigin::none(), SLOT_DURATION));
            Timestamp::on_finalize(1);
            Babe::on_finalize(1);
            let older_proof = finality_proof(&System::finalize());
            assert_eq!(
                GearEthBridge::verify_finality_proof(older_proof.clone()),
                Some(1)
            );

            initialize_block(2);
            Babe::on_initialize(2);
            assert_noop!(
                GearEthBridge::send_eth_message(
                    RuntimeOrigin::signed(alice.0.clone()),
                    sp_core::H160::repeat_byte(5),
                    vec![5],
                ),
                pallet_gear_eth_bridge::Error::<Runtime>::BridgeCleanupRequired
            );
            assert_ok!(GearEthBridge::send_eth_message(
                RuntimeOrigin::signed(admin.clone()),
                sp_core::H160::repeat_byte(6),
                vec![6],
            ));
            let (accepted, hash) = last_bridge_message();
            assert_eq!(accepted.nonce(), sp_core::U256::from(capacity));
            assert_noop!(
                GearEthBridge::reset_overflowed_queue(
                    RuntimeOrigin::signed(alice.0.clone()),
                    older_proof.clone(),
                ),
                pallet_gear_eth_bridge::Error::<Runtime>::InvalidQueueReset
            );
            assert_eq!(GearEthBridge::bridge_snapshot(), Some(original));
            assert_eq!(
                GearEthBridge::merkle_proof(hash).unwrap().number_of_leaves,
                u64::from(capacity) + 1
            );

            GearEthBridge::on_finalize(2);
            let accepted_proof = GearEthBridge::merkle_proof(hash).unwrap();
            assert_eq!(
                GearEthBridge::bridge_snapshot(),
                Some((original.0, accepted_proof.root))
            );
            assert_ne!(accepted_proof.root, original.1);
            assert_ok!(Timestamp::set(RuntimeOrigin::none(), 2 * SLOT_DURATION));
            Timestamp::on_finalize(2);
            Babe::on_finalize(2);
            let newer_proof = finality_proof(&System::finalize());
            assert_eq!(
                GearEthBridge::verify_finality_proof(older_proof.clone()),
                Some(1)
            );
            assert_eq!(
                GearEthBridge::verify_finality_proof(newer_proof.clone()),
                Some(2)
            );

            initialize_block(3);
            assert_noop!(
                GearEthBridge::reset_overflowed_queue(
                    RuntimeOrigin::signed(alice.0.clone()),
                    older_proof,
                ),
                pallet_gear_eth_bridge::Error::<Runtime>::InvalidQueueReset
            );
            assert_eq!(GearEthBridge::merkle_proof(hash), Some(accepted_proof));
            assert_ok!(GearEthBridge::reset_overflowed_queue(
                RuntimeOrigin::signed(alice.0),
                newer_proof,
            ));
            assert_eq!(
                GearEthBridge::bridge_snapshot(),
                Some((original.0 + 1, sp_core::H256::zero()))
            );
            assert!(GearEthBridge::merkle_proof(hash).is_none());
            assert_ok!(GearEthBridge::send_eth_message(
                RuntimeOrigin::signed(admin),
                sp_core::H160::repeat_byte(7),
                vec![7],
            ));
            assert_eq!(
                last_bridge_message().0.nonce(),
                sp_core::U256::from(capacity + 1)
            );
        });
}
