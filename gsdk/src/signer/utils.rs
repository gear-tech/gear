// This file is part of Gear.
//
// Copyright (C) 2021-2025 Gear Technologies Inc.
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

use super::Inner;
use crate::{
    TxInBlock, TxStatus,
    backtrace::BacktraceStatus,
    config::GearConfig,
    metadata::{
        CallInfo, Event, calls::SudoCall, sudo::Event as SudoEvent, vara_runtime::RuntimeCall,
    },
    result::Result,
    signer::SignerRpc,
};
use anyhow::anyhow;
use colored::Colorize;
use scale_value::Composite;
use sp_core::H256;
use std::sync::Arc;
use subxt::{
    Error as SubxtError, OnlineClient,
    blocks::ExtrinsicEvents,
    config::polkadot::PolkadotExtrinsicParamsBuilder,
    dynamic::Value,
    tx::{DynamicPayload, TxProgress},
};

type TxProgressT = TxProgress<GearConfig, OnlineClient<GearConfig>>;
pub type EventsResult = Result<(H256, ExtrinsicEvents<GearConfig>)>;

impl Inner {
    /// Logging balance spent
    pub async fn log_balance_spent(&self, before: u128) -> Result<()> {
        let signer_rpc = SignerRpc(Arc::new(self.clone()));
        match signer_rpc.get_balance().await {
            Ok(balance) => {
                let after = before.saturating_sub(balance);
                log::info!("\tBalance spent: {after}");
            }
            Err(e) => log::info!("\tAccount was removed from storage: {e}"),
        }

        Ok(())
    }

    /// Propagates log::info for given status.
    pub(crate) fn log_status(status: &TxStatus) {
        match status {
            TxStatus::Validated => log::info!("\tStatus: Validated"),
            TxStatus::Broadcasted { num_peers } => log::info!("\tStatus: Broadcast( {num_peers} )"),
            TxStatus::NoLongerInBestBlock => log::info!("\tStatus: NoLongerInBestBlock"),
            TxStatus::InBestBlock(b) => log::info!(
                "\tStatus: InBestBlock( block hash: {}, extrinsic hash: {} )",
                b.block_hash(),
                b.extrinsic_hash()
            ),
            TxStatus::InFinalizedBlock(b) => log::info!(
                "\tStatus: Finalized( block hash: {}, extrinsic hash: {} )",
                b.block_hash(),
                b.extrinsic_hash()
            ),
            TxStatus::Error { message: e } => log::error!("\tStatus: Error( {e:?} )"),
            TxStatus::Dropped { message: e } => log::error!("\tStatus: Dropped( {e:?} )"),
            TxStatus::Invalid { message: e } => log::error!("\tStatus: Invalid( {e:?} )"),
        }
    }

    /// Listen transaction process and print logs.
    pub async fn process(&self, tx: DynamicPayload) -> Result<TxInBlock> {
        use subxt::tx::TxStatus::*;

        let signer_rpc = SignerRpc(Arc::new(self.clone()));
        let before = signer_rpc.get_balance().await?;

        let mut process = self.sign_and_submit_then_watch(&tx).await?;
        let (pallet, name) = (tx.pallet_name(), tx.call_name());
        let extrinsic = format!("{pallet}::{name}").magenta().bold();

        log::info!("Pending {extrinsic} ...");
        let mut queue: Vec<BacktraceStatus> = Default::default();
        let mut hash: Option<H256> = None;

        while let Some(status) = process.next().await {
            let status = status?;
            Self::log_status(&status);

            if let Some(h) = &hash {
                self.backtrace
                    .clone()
                    .append(*h, BacktraceStatus::from(&status));
            } else {
                queue.push((&status).into());
            }

            match status {
                Validated | Broadcasted { .. } | NoLongerInBestBlock => (),
                InBestBlock(b) => {
                    hash = Some(b.extrinsic_hash());
                    self.backtrace.append(
                        b.extrinsic_hash(),
                        BacktraceStatus::InBestBlock {
                            block_hash: b.block_hash(),
                            extrinsic_hash: b.extrinsic_hash(),
                        },
                    );
                }
                InFinalizedBlock(b) => {
                    log::info!("Submitted {extrinsic} !");
                    log::info!("\tBlock Hash: {:?}", b.block_hash());
                    log::info!("\tTransaction Hash: {:?}", b.extrinsic_hash());
                    self.log_balance_spent(before).await?;
                    return Ok(b);
                }
                _ => {
                    self.log_balance_spent(before).await?;
                    return Err(status.into());
                }
            }
        }

        Err(anyhow!("Transaction wasn't found").into())
    }

    /// Process sudo transaction.
    pub async fn process_sudo(&self, tx: DynamicPayload) -> EventsResult {
        let tx = self.process(tx).await?;
        let events = tx.wait_for_success().await?;
        for event in events.iter() {
            let event = event?.as_root_event::<Event>()?;
            if let Event::Sudo(SudoEvent::Sudid {
                sudo_result: Err(err),
            }) = event
            {
                return Err(self.api().decode_error(err).into());
            }
        }

        Ok((tx.block_hash(), events))
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
    ///     let in_block = signer.run_tx(BalancesCall::TransferKeepAlive, args).await?;
    /// }
    ///
    /// // The code above euqals to:
    ///
    /// {
    ///    let in_block = signer.calls.transfer_keep_alive(dest, value).await?;
    /// }
    ///
    /// // ...
    /// ```
    pub async fn run_tx<Call: CallInfo>(
        &self,
        call: Call,
        fields: impl Into<Composite<()>>,
    ) -> Result<TxInBlock> {
        let tx = subxt::dynamic::tx(Call::PALLET, call.call_name(), fields.into());

        self.process(tx).await
    }

    /// Run transaction with sudo.
    pub async fn sudo_run_tx<Call: CallInfo>(
        &self,
        call: Call,
        fields: impl Into<Composite<()>>,
    ) -> EventsResult {
        let tx = subxt::dynamic::tx(Call::PALLET, call.call_name(), fields.into());

        self.process_sudo(tx).await
    }

    /// `pallet_sudo::sudo`
    pub async fn sudo(&self, call: RuntimeCall) -> EventsResult {
        self.sudo_run_tx(SudoCall::Sudo, vec![Value::from(call)])
            .await
    }

    /// Wrapper for submit and watch with nonce.
    async fn sign_and_submit_then_watch(
        &self,
        tx: &DynamicPayload,
    ) -> Result<TxProgressT, SubxtError> {
        if let Some(nonce) = self.nonce {
            self.api
                .tx()
                .create_signed(
                    tx,
                    &self.signer,
                    PolkadotExtrinsicParamsBuilder::new().nonce(nonce).build(),
                )
                .await?
                .submit_and_watch()
                .await
        } else {
            self.api
                .tx()
                .sign_and_submit_then_watch_default(tx, &self.signer)
                .await
        }
    }
}
