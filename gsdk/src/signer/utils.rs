// This file is part of Gear.
//
// Copyright (C) 2021-2023 Gear Technologies Inc.
// SPDX-License-Identifier: GPL-3.0-or-later WITH Classpath-exception-2.0
//
// This program is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.
//
// This program is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE. See the
// GNU General Public License for more details.
//
// You should have received a copy of the GNU General Public License
// along with this program. If not, see <https://www.gnu.org/licenses/>.

//! Utils

use crate::{
    config::GearConfig, metadata::CallInfo, result::Result, signer::Signer, types::InBlock, Error,
};
use scale_value::Composite;
use subxt::blocks::ExtrinsicEvents;

type EventsResult = Result<ExtrinsicEvents<GearConfig>, Error>;

impl Signer {
    /// Get self balance
    pub async fn balance(&self) -> Result<u128> {
        self.api().get_balance(&self.address()).await
    }

    /// Logging balance spent
    pub async fn log_balance_spent(&self, before: u128) -> Result<()> {
        let after = before.saturating_sub(self.balance().await?);
        log::info!("	Balance spent: {after}");

        Ok(())
    }

    /// Run transaction.
    pub async fn run_tx<'a, Call: CallInfo>(
        &self,
        call: Call,
        fields: impl Into<Composite<()>>,
    ) -> InBlock {
        let tx = subxt::dynamic::tx(Call::PALLET, call.call_name(), fields.into());

        self.process(tx).await
    }

    /// Run transaction with sudo.
    pub async fn sudo_run_tx<'a, Call: CallInfo>(
        &self,
        call: Call,
        fields: impl Into<Composite<()>>,
    ) -> EventsResult {
        let tx = subxt::dynamic::tx(Call::PALLET, call.call_name(), fields.into());

        self.process_sudo(tx).await
    }
}
