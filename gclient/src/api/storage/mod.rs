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

mod block;

pub use block::*;

use super::{GearApi, Result};
use crate::Error;
use gear_core::{ids::*, message::StoredMessage};
use gp::api::generated::api::runtime_types::gear_common::storage::primitives::Interval;
use subxt::sp_runtime::AccountId32;

impl GearApi {
    pub async fn get_from_mailbox(
        &self,
        account_id: AccountId32,
        message_id: MessageId,
    ) -> Result<Option<(StoredMessage, Interval<u32>)>> {
        let data = self
            .0
            .storage()
            .gear_messenger()
            .mailbox(&account_id, &message_id.into(), None)
            .await?;

        Ok(data.map(|(m, i)| (m.into(), i)))
    }

    pub async fn total_balance(&self, account_id: AccountId32) -> Result<u128> {
        let data = self
            .0
            .storage()
            .balances()
            .account(&account_id, None)
            .await?;

        data.free
            .checked_add(data.reserved)
            .ok_or(Error::BalanceOverflow)
    }

    pub async fn free_balance(&self, account_id: AccountId32) -> Result<u128> {
        let data = self
            .0
            .storage()
            .balances()
            .account(&account_id, None)
            .await?;

        Ok(data.free)
    }

    pub async fn reserved_balance(&self, account_id: AccountId32) -> Result<u128> {
        let data = self
            .0
            .storage()
            .balances()
            .account(&account_id, None)
            .await?;

        Ok(data.free)
    }
}
