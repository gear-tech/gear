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

#![allow(unused)]

use crate as pallet_gear_voucher;
use common::{
    storage::{Interval, Mailbox},
    Origin,
};
use frame_support::{
    construct_runtime, parameter_types,
    weights::{constants::RocksDbWeight, Weight},
    PalletId,
};
use frame_system as system;
use gear_core::{
    ids::{MessageId, ProgramId},
    message::UserStoredMessage,
};
use primitive_types::H256;
use sp_core::ConstU8;
use sp_runtime::{
    generic,
    traits::{BlakeTwo256, IdentityLookup, Zero},
};
use sp_std::convert::{TryFrom, TryInto};

type UncheckedExtrinsic = frame_system::mocking::MockUncheckedExtrinsic<Test>;
type Block = frame_system::mocking::MockBlock<Test>;
type AccountId = u64;
type BlockNumber = u64;
type Balance = u128;

pub const ALICE: AccountId = 1;
pub const BOB: AccountId = 2;

// Configure a mock runtime to test the pallet.
construct_runtime!(
    pub enum Test where
        Block = Block,
        NodeBlock = Block,
        UncheckedExtrinsic = UncheckedExtrinsic,
    {
        System: system,
        Voucher: pallet_gear_voucher,
        Balances: pallet_balances,
    }
);

parameter_types! {
    pub const BlockHashCount: BlockNumber = 250;
    pub const ExistentialDeposit: Balance = 1;
}

common::impl_pallet_system!(Test, DbWeight = RocksDbWeight, BlockWeights = ());
common::impl_pallet_balances!(Test);

parameter_types! {
    pub const VoucherPalletId: PalletId = PalletId(*b"py/vouch");
}

impl crate::PrepaidCallsDispatcher for () {
    type AccountId = AccountId;
    type Balance = Balance;

    fn weight(_call: &pallet_gear_voucher::PrepaidCall<Balance>) -> Weight {
        Zero::zero()
    }
    fn dispatch(
        _account_id: Self::AccountId,
        _sponsor_id: Self::AccountId,
        _call: pallet_gear_voucher::PrepaidCall<Balance>,
    ) -> frame_support::pallet_prelude::DispatchResultWithPostInfo {
        Ok(().into())
    }
}

pub const MAILBOXED_PROGRAM: ProgramId = ProgramId::test_new([0; 32]);
pub const MAILBOXED_MESSAGE: MessageId = MessageId::test_new([0; 32]);

pub struct MailboxMock;

impl Mailbox for MailboxMock {
    type BlockNumber = ();
    type Error = ();
    type Key1 = AccountId;
    type Key2 = MessageId;
    type Value = UserStoredMessage;
    type OutputError = ();

    fn clear() {
        unimplemented!()
    }
    fn contains(_key1: &Self::Key1, _key2: &Self::Key2) -> bool {
        unimplemented!()
    }
    fn insert(_value: Self::Value, _bn: Self::BlockNumber) -> Result<(), Self::OutputError> {
        unimplemented!()
    }
    fn peek(key1: &Self::Key1, key2: &Self::Key2) -> Option<Self::Value> {
        if *key2 == MAILBOXED_MESSAGE {
            Some(UserStoredMessage::new(
                MAILBOXED_MESSAGE,
                MAILBOXED_PROGRAM,
                (*key1).cast(),
                vec![].try_into().unwrap(),
                0,
            ))
        } else {
            None
        }
    }
    fn remove(
        _key1: Self::Key1,
        _key2: Self::Key2,
    ) -> Result<(Self::Value, Interval<Self::BlockNumber>), Self::OutputError> {
        unimplemented!()
    }
}

impl pallet_gear_voucher::Config for Test {
    type RuntimeEvent = RuntimeEvent;
    type Currency = Balances;
    type PalletId = VoucherPalletId;
    type WeightInfo = ();
    type CallsDispatcher = ();
    type Mailbox = MailboxMock;
    type MaxProgramsAmount = ConstU8<3>;
}

// Build genesis storage according to the mock runtime.
pub fn new_test_ext() -> sp_io::TestExternalities {
    let mut t = system::GenesisConfig::default()
        .build_storage::<Test>()
        .unwrap();

    pallet_balances::GenesisConfig::<Test> {
        balances: vec![(ALICE, 100_000_000_u128), (BOB, 100_u128)],
    }
    .assimilate_storage(&mut t)
    .unwrap();

    let mut ext = sp_io::TestExternalities::new(t);
    ext.execute_with(|| System::set_block_number(1));
    ext
}
