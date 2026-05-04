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

#![allow(unused)]

use crate::{self as pallet_gear_voucher, VoucherId};
use common::{
    Origin,
    storage::{Interval, Mailbox},
};
use frame_support::{
    PalletId, construct_runtime, parameter_types,
    traits::ConstU32,
    weights::{Weight, constants::RocksDbWeight},
};
use frame_system::{self as system, pallet_prelude::BlockNumberFor};
use gear_core::{
    ids::{ActorId, MessageId},
    message::UserStoredMessage,
};
use primitive_types::H256;
use sp_core::ConstU8;
use sp_runtime::{
    BuildStorage,
    traits::{BlakeTwo256, IdentityLookup, Zero},
};
use sp_std::convert::{TryFrom, TryInto};

type Block = frame_system::mocking::MockBlock<Test>;
type AccountId = u64;
pub type BlockNumber = BlockNumberFor<Test>;
type Balance = u128;

pub const ALICE: AccountId = 1;
pub const BOB: AccountId = 2;

// Configure a mock runtime to test the pallet.
construct_runtime!(
    pub enum Test
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
    pub const MinVoucherDuration: BlockNumber = 5;
    pub const MaxVoucherDuration: BlockNumber = 100_000_000;
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
        _voucher_id: VoucherId,
        _call: pallet_gear_voucher::PrepaidCall<Balance>,
    ) -> frame_support::pallet_prelude::DispatchResultWithPostInfo {
        Ok(().into())
    }
}

pub const MAILBOXED_PROGRAM: ActorId = ActorId::zero();
pub const MAILBOXED_MESSAGE: MessageId = MessageId::zero();

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
        (*key2 == MAILBOXED_MESSAGE).then(|| {
            UserStoredMessage::new(
                MAILBOXED_MESSAGE,
                MAILBOXED_PROGRAM,
                (*key1).cast(),
                vec![].try_into().unwrap(),
                0,
            )
        })
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
    type MaxDuration = MaxVoucherDuration;
    type MinDuration = MinVoucherDuration;
}

// Build genesis storage according to the mock runtime.
pub fn new_test_ext() -> sp_io::TestExternalities {
    let mut t = system::GenesisConfig::<Test>::default()
        .build_storage()
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
