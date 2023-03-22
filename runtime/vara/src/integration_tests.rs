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

use crate::*;
use frame_support::{
    assert_noop, assert_ok,
    traits::{GenesisBuild, OnFinalize, OnInitialize},
};
use frame_system::pallet_prelude::BlockNumberFor;
use sp_consensus_babe::{
    digests::{PreDigest, SecondaryPlainPreDigest},
    Slot, BABE_ENGINE_ID,
};
use sp_core::{ed25519, sr25519, Pair};
use sp_keyring::{AccountKeyring, Sr25519Keyring};
use sp_runtime::{traits::SignedExtension, Digest, DigestItem, MultiAddress};

const ENDOWMENT: u128 = 100 * UNITS;
const STASH: u128 = 10 * UNITS;

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
    GearProgram::on_initialize(new_block_number);
    GearMessenger::on_initialize(new_block_number);
    Gear::on_initialize(new_block_number);
    GearGas::on_initialize(new_block_number);
    // Session::on_initialize(new_block_number);
}

// Run on_finalize hooks (in pallets reverse order, as they appear in AllPalletsWithSystem)
pub(crate) fn on_finalize(current_blk: BlockNumberFor<Runtime>) {
    Gear::run(frame_support::dispatch::RawOrigin::None.into()).unwrap();
    GearPayment::on_finalize(current_blk);
    GearGas::on_finalize(current_blk);
    Gear::on_finalize(current_blk);
    GearMessenger::on_finalize(current_blk);
    GearProgram::on_finalize(current_blk);
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
        let mut storage = frame_system::GenesisConfig::default()
            .build_storage::<Runtime>()
            .unwrap();

        pallet_balances::GenesisConfig::<Runtime> {
            balances: self
                .initial_authorities
                .iter()
                .map(|x| (x.0.clone(), self.stash))
                .chain(
                    self.endowed_accounts
                        .iter()
                        .map(|k| (k.clone(), self.endowment)),
                )
                .collect(),
        }
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
        }
        .assimilate_storage(&mut storage)
        .unwrap();

        SudoConfig { key: self.root }
            .assimilate_storage(&mut storage)
            .unwrap();

        GenesisBuild::<Runtime>::assimilate_storage(
            &VestingConfig {
                vesting: self.vested_accounts,
            },
            &mut storage,
        )
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
    let _ = env_logger::Builder::from_default_env()
        .format_module_path(false)
        .format_level(true)
        .try_init();
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

#[test]
fn signed_extension_works() {
    use sp_runtime::transaction_validity::{InvalidTransaction, TransactionSource};

    init_logger();

    let alice = AccountKeyring::Alice;
    let bob = AccountKeyring::Bob;
    let charlie = AccountKeyring::Charlie;
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
        .endowed_accounts(vec![charlie.into()])
        .vested_accounts(vec![(charlie.into(), 100, 500, 10 * UNITS)])
        .build()
        .execute_with(|| {
            let construct_extrinsic =
                |function: RuntimeCall, caller: Sr25519Keyring| -> UncheckedExtrinsic {
                    let extra: SignedExtra = (
                        DisableValueTransfers {},
                        StakingBlackList::<Runtime>::new(),
                        frame_system::CheckNonZeroSender::<Runtime>::new(),
                        frame_system::CheckSpecVersion::<Runtime>::new(),
                        frame_system::CheckTxVersion::<Runtime>::new(),
                        frame_system::CheckGenesis::<Runtime>::new(),
                        frame_system::CheckEra::<Runtime>::from(generic::Era::mortal(256, 0)),
                        frame_system::CheckNonce::<Runtime>::from(0),
                        frame_system::CheckWeight::<Runtime>::new(),
                        CustomChargeTransactionPayment::<Runtime>::from(0),
                    );
                    UncheckedExtrinsic::new_signed(
                        function.clone(),
                        MultiAddress::Id(caller.public().into()),
                        Signature::Sr25519(
                            SignedPayload::from_raw(
                                function,
                                extra.clone(),
                                extra.additional_signed().unwrap(),
                            )
                            .using_encoded(|e| caller.sign(e)),
                        ),
                        extra,
                    )
                };

            // Balance transfer should be blocked by the signed extension
            let invalid_call = construct_extrinsic(
                RuntimeCall::Balances(pallet_balances::Call::transfer {
                    dest: MultiAddress::Id(bob.into()),
                    value: 10 * UNITS,
                }),
                alice,
            );

            // Wrapping `bond` call in a batch is also illegal
            let invalid_batch = construct_extrinsic(
                RuntimeCall::Utility(pallet_utility::Call::batch {
                    calls: vec![RuntimeCall::Balances(pallet_balances::Call::transfer {
                        dest: MultiAddress::Id(bob.into()),
                        value: 10 * UNITS,
                    })],
                }),
                alice,
            );

            let invalid_batch_all = construct_extrinsic(
                RuntimeCall::Utility(pallet_utility::Call::batch_all {
                    calls: vec![RuntimeCall::Balances(pallet_balances::Call::transfer {
                        dest: MultiAddress::Id(bob.into()),
                        value: 10 * UNITS,
                    })],
                }),
                alice,
            );

            // Nested batches and/or other `Utility` calls shouldn't work, as well
            let nested_batches = construct_extrinsic(
                RuntimeCall::Utility(pallet_utility::Call::batch {
                    calls: vec![RuntimeCall::Utility(pallet_utility::Call::batch_all {
                        calls: vec![RuntimeCall::Utility(pallet_utility::Call::as_derivative {
                            index: 0,
                            call: Box::new(RuntimeCall::Balances(
                                pallet_balances::Call::transfer {
                                    dest: MultiAddress::Id(bob.into()),
                                    value: 10 * UNITS,
                                },
                            )),
                        })],
                    })],
                }),
                alice,
            );

            let valid_call =
                construct_extrinsic(RuntimeCall::Vesting(pallet_vesting::Call::vest {}), charlie);

            assert_eq!(
                Executive::validate_transaction(
                    TransactionSource::External,
                    invalid_call,
                    Default::default(),
                )
                .unwrap_err(),
                InvalidTransaction::Call.into()
            );

            assert_eq!(
                Executive::validate_transaction(
                    TransactionSource::External,
                    invalid_batch,
                    Default::default(),
                )
                .unwrap_err(),
                InvalidTransaction::Call.into()
            );

            assert_eq!(
                Executive::validate_transaction(
                    TransactionSource::External,
                    invalid_batch_all,
                    Default::default(),
                )
                .unwrap_err(),
                InvalidTransaction::Call.into()
            );

            assert_eq!(
                Executive::validate_transaction(
                    TransactionSource::External,
                    nested_batches,
                    Default::default(),
                )
                .unwrap_err(),
                InvalidTransaction::Call.into()
            );

            assert_ok!(Executive::validate_transaction(
                TransactionSource::External,
                valid_call,
                Default::default(),
            ));
        });
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
            (dave.into(), 10, 100, 10 * UNITS), // 1 TOKEN unlocked per block
            (eve.into(), 10, 100, 10 * UNITS),
            (ferdie.into(), 10, 100, 10 * UNITS),
        ])
        .root(alice.into())
        .build()
        .execute_with(|| {
            let acc_data = System::account(dave.to_account_id()).data;
            // Free balance of vested accounts is still 100 TOKENS
            assert_eq!(acc_data.free, 100 * UNITS);
            // Locked balance is 90 TOKENS
            assert_eq!(acc_data.misc_frozen, 90 * UNITS);

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
                    10 * UNITS,
                ),
                pallet_gear::Error::<Runtime>::InsufficientBalanceForReserve
            );

            // Locked funds can't be transferred to a program as a message `value`
            assert_ok!(Gear::upload_program(
                RuntimeOrigin::signed(alice.to_account_id()),
                code,
                b"salt".to_vec(),
                vec![],
                10_000_000_000,
                0,
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
                    11 * UNITS,
                ),
                pallet_gear::Error::<Runtime>::InsufficientBalanceForReserve
            );
        });
}
