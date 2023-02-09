// This file is part of Gear.

// Copyright (C) 2022 Gear Technologies Inc.
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

mod account_id;
mod block;

pub use block::*;

use super::{GearApi, Result};
use crate::Error;
use account_id::IntoAccountId32;
use gear_core::{ids::*, message::StoredMessage};
use gp::api::generated::api::{
    runtime_types::{
        gear_common::storage::primitives::Interval, gear_core::ids as generated_ids,
        pallet_balances::AccountData,
    },
    storage,
};

impl GearApi {
    /// Get a message identified by `message_id` from the mailbox.
    pub async fn get_from_mailbox(
        &self,
        message_id: MessageId,
    ) -> Result<Option<(StoredMessage, Interval<u32>)>> {
        self.get_from_account_mailbox(self.0.account_id(), message_id)
            .await
    }

    /// Get a message identified by `message_id` from the `account_id`'s
    /// mailbox.
    pub async fn get_from_account_mailbox(
        &self,
        account_id: impl IntoAccountId32,
        message_id: MessageId,
    ) -> Result<Option<(StoredMessage, Interval<u32>)>> {
        let at = storage().gear_messenger().mailbox(
            account_id.into_account_id(),
            generated_ids::MessageId::from(message_id),
        );
        let data = self.0.storage().fetch(&at, None).await?;

        Ok(data.map(|(m, i)| (m.into(), i)))
    }

    async fn account_data(&self, account_id: impl IntoAccountId32) -> Result<AccountData<u128>> {
        let at = storage().system().account(account_id.into_account_id());

        let data = self
            .0
            .storage()
            .fetch(&at, None)
            .await?
            .map(|v| v.data)
            .unwrap_or(AccountData {
                free: 0,
                reserved: 0,
                misc_frozen: 0,
                fee_frozen: 0,
            });

        Ok(data)
    }

    /// Get the total balance of the account identified by `account_id`.
    ///
    /// Total balance includes free and reserved funds.
    pub async fn total_balance(&self, account_id: impl IntoAccountId32) -> Result<u128> {
        let data = self.account_data(account_id).await?;

        data.free
            .checked_add(data.reserved)
            .ok_or(Error::BalanceOverflow)
    }

    /// Get the free funds balance of the account identified by `account_id`.
    pub async fn free_balance(&self, account_id: impl IntoAccountId32) -> Result<u128> {
        let data = self.account_data(account_id).await?;

        Ok(data.free)
    }

    /// Get the reserved funds balance of the account identified by
    /// `account_id`.
    pub async fn reserved_balance(&self, account_id: impl IntoAccountId32) -> Result<u128> {
        let data = self.account_data(account_id).await?;

        Ok(data.reserved)
    }
}
