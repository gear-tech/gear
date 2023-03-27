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
    ext::sp_core::{crypto::Ss58Codec, H256},
    metadata::runtime_types::{
        gear_common::storage::primitives::Interval, gear_core::message::stored,
        pallet_balances::AccountData,
    },
};

impl GearApi {
    /// Get a message identified by `message_id` from the mailbox.
    pub async fn get_mailbox_message(
        &self,
        message_id: MessageId,
    ) -> Result<Option<(StoredMessage, Interval<u32>)>> {
        self.get_mailbox_account_message(self.0.account_id(), message_id)
            .await
    }

    /// Get a message identified by `message_id` from the `account_id`'s
    /// mailbox.
    pub async fn get_mailbox_account_message(
        &self,
        account_id: impl IntoAccountId32,
        message_id: MessageId,
    ) -> Result<Option<(StoredMessage, Interval<u32>)>> {
        let data: Option<(stored::StoredMessage, Interval<u32>)> = self
            .0
            .api()
            .get_mailbox_account_message(account_id.into_account_id(), message_id)
            .await?;
        Ok(data.map(|(m, i)| (m.into(), i)))
    }

    /// Get up to `count` messages from the mailbox from
    /// the provided `account_id`.
    pub async fn get_mailbox_account_messages(
        &self,
        account_id: impl IntoAccountId32,
        count: u32,
    ) -> Result<Vec<(StoredMessage, Interval<u32>)>> {
        let data = self
            .0
            .api()
            .mailbox(Some(account_id.into_account_id()), count)
            .await?;
        Ok(data.into_iter().map(|(m, i)| (m.into(), i)).collect())
    }

    /// Get up to `count` messages from the mailbox.
    pub async fn get_mailbox_messages(
        &self,
        count: u32,
    ) -> Result<Vec<(StoredMessage, Interval<u32>)>> {
        self.get_mailbox_account_messages(self.0.account_id(), count)
            .await
    }

    /// Get account data by `account_id`.
    pub(crate) async fn account_data(
        &self,
        account_id: impl IntoAccountId32,
    ) -> Result<AccountData<u128>> {
        self.account_data_at(account_id, None).await
    }

    /// Get account data by `account_id` at specified block.
    pub(crate) async fn account_data_at(
        &self,
        account_id: impl IntoAccountId32,
        block_hash: Option<H256>,
    ) -> Result<AccountData<u128>> {
        Ok(self
            .0
            .api()
            .info_at(&account_id.into_account_id().to_ss58check(), block_hash)
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
