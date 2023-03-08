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

pub(crate) mod account_id;
mod block;

pub use block::*;

use super::{GearApi, Result};
use crate::Error;
use account_id::IntoAccountId32;
use gear_core::{ids::*, message::StoredMessage};
use gsdk::{
    ext::sp_core::crypto::Ss58Codec,
    metadata::runtime_types::{
        gear_common::storage::primitives::Interval, gear_core::message::stored,
        pallet_balances::AccountData,
    },
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
        let data: Option<(stored::StoredMessage, Interval<u32>)> = self
            .0
            .api()
            .get_from_account_mailbox(account_id.into_account_id(), message_id)
            .await?;
        Ok(data.map(|(m, i)| (m.into(), i)))
    }

    async fn account_data(&self, account_id: impl IntoAccountId32) -> Result<AccountData<u128>> {
        Ok(self
            .0
            .api()
            .info(&account_id.into_account_id().to_ss58check())
            .await?
            .data)
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
