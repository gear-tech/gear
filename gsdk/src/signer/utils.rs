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

use std::sync::Arc;

use super::SignerInner;
use crate::{
    config::GearConfig, metadata::CallInfo, result::Result, signer::SignerRpc, Error, TxInBlock,
};
use scale_value::Composite;
use subxt::blocks::ExtrinsicEvents;

type EventsResult = Result<ExtrinsicEvents<GearConfig>, Error>;

impl SignerInner {
    /// Logging balance spent
    pub async fn log_balance_spent(&self, before: u128) -> Result<()> {
        let signer_rpc = SignerRpc(Arc::new(self.clone()));
        let after = before.saturating_sub(signer_rpc.get_balance().await?);
        log::info!("	Balance spent: {after}");

        Ok(())
    }

    /// Run transaction.
    ///
    /// This function allows us to execute any transactions in gear.
    ///
    /// # You may not need this.
    ///
    /// Read the docs of [`Signer`](`super::Signer`) to checkout the wrappred transactions,
    /// we need this function only when we want to execute a transaction
    /// which has not been wrapped in `gsdk`.
    ///
    /// # Example
    ///
    /// ```ignore
    /// use gsdk::{
    ///   Api,
    ///   Signer,
    ///   metadata::calls::BalancesCall,
    ///   Value,
    /// };
    ///
    /// let api = Api::new(None).await?;
    /// let signer = Signer::new(api, "//Alice", None).await?;
    ///
    /// {
    ///     let args = vec![
    ///         Value::unnamed_variant("Id", [Value::from_bytes(dest.into())]),
    ///         Value::u128(value),
    ///     ];
    ///     let in_block = signer.run_tx(BalancesCall::Transfer, args).await?;
    /// }
    ///
    /// // The code above euqals to:
    ///
    /// {
    ///    let in_block = signer.calls.transfer(dest, value).await?;
    /// }
    ///
    /// // ...
    /// ```
    pub async fn run_tx<'a, Call: CallInfo>(
        &self,
        call: Call,
        fields: impl Into<Composite<()>>,
    ) -> Result<TxInBlock> {
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
