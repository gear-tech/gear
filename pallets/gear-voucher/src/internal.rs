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

use crate::*;
use common::storage::{Counter, CounterImpl, Mailbox};
use gear_core::{declare_id, ids};

declare_id!(VoucherId: "Voucher identifier");

impl VoucherId {
    pub fn generate<T: Config>() -> Self {
        const SALT: &[u8] = b"voucher";

        let nonce = CounterImpl::<u64, IssuedWrap<T>>::inc_get();

        let argument = [SALT, &nonce.to_le_bytes()].concat();
        ids::hash(&argument).into()
    }
}

/// Type containing all data about voucher.
#[derive(Debug, Encode, Decode, TypeInfo)]
pub struct VoucherInfo<AccountId, Balance, BlockNumber> {
    pub owner: AccountId,
    pub spender: AccountId,
    pub balance: Balance,
    pub max_value: Option<Balance>,
    pub valid_until: BlockNumber,
    pub destinations: Vec<ProgramId>,
}

/// Prepaid call to be executed on-chain.
#[derive(Debug, Clone, Encode, Decode, TypeInfo, PartialEq, Eq, PartialOrd, Ord)]
pub enum PrepaidCall<Balance> {
    SendMessage {
        destination: ProgramId,
        payload: Vec<u8>,
        gas_limit: u64,
        value: Balance,
        keep_alive: bool,
    },
    SendReply {
        reply_to_id: MessageId,
        payload: Vec<u8>,
        gas_limit: u64,
        value: Balance,
        keep_alive: bool,
    },
}

/// Trait for processing prepaid calls by any implementor.
pub trait PrepaidCallsDispatcher {
    type AccountId;
    type Balance;

    /// Returns weight of processing for call.
    fn weight(call: &PrepaidCall<Self::Balance>) -> Weight;

    /// Processes prepaid call with specific sponsor from origins address.
    fn dispatch(
        account_id: Self::AccountId,
        sponsor_id: Self::AccountId,
        call: PrepaidCall<Self::Balance>,
    ) -> DispatchResultWithPostInfo;
}

impl<T: Config> Pallet<T> {
    /// Derive a synthesized account ID from an account ID and a program ID.
    pub fn voucher_id(who: &T::AccountId, program_id: &ProgramId) -> T::AccountId {
        let entropy = (b"modlpy/voucher__", who, program_id).using_encoded(blake2_256);
        Decode::decode(&mut TrailingZeroInput::new(entropy.as_ref()))
            .expect("infinite length input; no invalid inputs for type; qed")
    }

    /// Return synthesized account ID based on call data.
    pub fn sponsor_of(
        who: &T::AccountId,
        call: &PrepaidCall<BalanceOf<T>>,
    ) -> Option<T::AccountId> {
        match call {
            PrepaidCall::SendMessage { destination, .. } => {
                Some(Self::voucher_id(who, destination))
            }
            PrepaidCall::SendReply { reply_to_id, .. } => T::Mailbox::peek(who, reply_to_id)
                .map(|stored_message| Self::voucher_id(who, &stored_message.source())),
        }
    }
}
