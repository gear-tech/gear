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

//! gear api calls
use super::Inner;
use crate::{
    AsGear, Error, Event, IntoAccountId32, Result, TxInBlock,
    gear::{
        self,
        runtime_types::{
            gear_common::event::MessageEntry,
            pallet_gear_voucher::internal::{PrepaidCall, VoucherId},
            sp_weights::weight_v2::Weight,
            vara_runtime::RuntimeCall,
        },
    },
    signer::utils::EventsResult,
};
use gear_core::ids::{ActorId, CodeId, MessageId};
use parity_scale_codec::Encode;
use subxt::utils::H256;

/// Implementation of calls to programs/other users for [`Signer`].
#[derive(Clone)]
pub struct SignerCalls<'a>(pub(crate) &'a Inner);

// pallet-balances
impl SignerCalls<'_> {
    /// Transfer `value` to `destination`'s account, but ensures
    /// that the original account won't be killed by the transfer
    /// transaction.
    ///
    /// Sends the
    /// [`pallet_balances::transfer_keep_alive`](https://crates.parity.io/pallet_balances/pallet/struct.Pallet.html#method.transfer_keep_alive)
    /// extrinsic.
    ///
    /// This function returns a hash of the block with the transfer transaction.
    pub async fn transfer_keep_alive(
        &self,
        dest: impl IntoAccountId32,
        value: u128,
    ) -> Result<H256> {
        let tx = self
            .0
            .run_tx(
                gear::tx()
                    .balances()
                    .transfer_keep_alive(dest.into_account_id().into(), value),
            )
            .await?;

        for event in tx.wait_for_success().await?.iter() {
            if let Event::Balances(gear::balances::Event::Transfer { .. }) = event?.as_gear()? {
                return Ok(tx.block_hash());
            }
        }

        // Sending zero value is a no-op, so now event occurs.
        if value == 0 {
            return Ok(tx.block_hash());
        }

        Err(Error::EventNotFound)
    }

    /// Transfer `value` to `destination`'s account.
    ///
    /// Sends the
    /// [`pallet_balances::transfer`](https://crates.parity.io/pallet_balances/pallet/struct.Pallet.html#method.transfer)
    /// extrinsic.
    ///
    /// This function returns a hash of the block with the transfer transaction.
    pub async fn transfer_allow_death(
        &self,
        dest: impl IntoAccountId32,
        value: u128,
    ) -> Result<H256> {
        let tx = self
            .0
            .run_tx(
                gear::tx()
                    .balances()
                    .transfer_allow_death(dest.into_account_id().into(), value),
            )
            .await?;

        for event in tx.wait_for_success().await?.iter() {
            if let Event::Balances(gear::balances::Event::Transfer { .. }) = event?.as_gear()? {
                return Ok(tx.block_hash());
            }
        }

        // Sending zero value is a no-op, so now event occurs.
        if value == 0 {
            return Ok(tx.block_hash());
        }

        Err(Error::EventNotFound)
    }

    /// Transfer the entire transferable balance from the caller to `destination`'s account.
    ///
    /// Sends the
    /// [`pallet_balances::transfer_all`](https://crates.parity.io/pallet_balances/pallet/struct.Pallet.html#method.transfer_all)
    /// extrinsic.
    ///
    /// This function returns a hash of the block with the transfer transaction.
    pub async fn transfer_all(&self, dest: impl IntoAccountId32, keep_alive: bool) -> Result<H256> {
        let tx = self
            .0
            .run_tx(
                gear::tx()
                    .balances()
                    .transfer_all(dest.into_account_id().into(), keep_alive),
            )
            .await?;

        for event in tx.wait_for_success().await?.iter() {
            if let Event::Balances(gear::balances::Event::Transfer { .. }) = event?.as_gear()? {
                return Ok(tx.block_hash());
            }
        }

        Err(Error::EventNotFound)
    }
}

// pallet-gear
impl SignerCalls<'_> {
    /// Create a new program from a previously uploaded code identified by
    /// [`CodeId`](https://docs.rs/gear_core/ids/struct.CodeId.html) and
    /// initialize it with a byte slice `payload`.
    ///
    /// Sends the
    /// [`pallet_gear::create_program`](https://docs.rs/pallet_gear/pallet/struct.Pallet.html#method.create_program)
    /// extrinsic.
    ///
    /// Parameters:
    ///
    /// - `code_id` is the code identifier that can be obtained by calling the
    ///   [`upload_code`](Self::upload_code) function;
    /// - `salt` is the arbitrary data needed to generate an address for a new
    ///   program (control of salt uniqueness is entirely on the function
    ///   caller’s side);
    /// - `payload` vector contains data to be processed in the `init` function
    ///   of the newly deployed "child" program;
    /// - `gas_limit` is the maximum gas amount allowed to spend for the program
    ///   creation and initialization;
    /// - `value` to be transferred to the program's account during
    ///   initialization.
    ///
    /// This function returns a tuple with an init message identifier, newly
    /// created program identifier, and a hash of the block with the message
    /// enqueuing transaction.
    ///
    /// # See also
    ///
    /// - [`create_program`](Self::create_program) function initializes a newly
    ///   created program with an encoded payload.
    /// - [`create_program_bytes_batch`](Self::create_program_bytes_batch)
    ///   function creates a batch of programs and initializes them.
    /// - [`upload_code`](Self::upload_code) function uploads a code and returns
    ///   its identifier.
    /// - [`upload_program_bytes`](Self::upload_program_bytes) function uploads
    ///   a new program and initialize it.
    pub async fn create_program_bytes(
        &self,
        code_id: CodeId,
        salt: impl Into<Vec<u8>>,
        payload: impl Into<Vec<u8>>,
        gas_limit: u64,
        value: u128,
    ) -> Result<(MessageId, ActorId, H256)> {
        let salt = salt.into();
        let payload = payload.into();

        let tx = self
            .0
            .run_tx(
                gear::tx()
                    .gear()
                    .create_program(code_id, salt, payload, gas_limit, value, false),
            )
            .await?;

        for event in tx.wait_for_success().await?.iter() {
            if let Event::Gear(gear::gear::Event::MessageQueued {
                id,
                destination,
                entry: MessageEntry::Init,
                ..
            }) = event?.as_gear()?
            {
                return Ok((id, destination, tx.block_hash()));
            }
        }

        Err(Error::EventNotFound)
    }

    /// Create a batch of programs.
    ///
    /// A batch is a set of programs to be created within one function call.
    /// Every entry of the `args` iterator is a tuple of parameters used in the
    /// [`create_program_bytes`](Self::create_program_bytes) function. It is
    /// useful when deploying a multi-program dApp.
    pub async fn create_program_bytes_batch(
        &self,
        args: impl IntoIterator<Item = (CodeId, impl Into<Vec<u8>>, impl Into<Vec<u8>>, u64, u128)>,
    ) -> Result<(Vec<Result<(MessageId, ActorId)>>, H256)> {
        let calls = args
            .into_iter()
            .map(|(code_id, salt, payload, gas_limit, value)| {
                RuntimeCall::Gear(gear::gear::Call::create_program {
                    code_id,
                    salt: salt.into(),
                    init_payload: payload.into(),
                    gas_limit,
                    value,
                    keep_alive: false,
                })
            })
            .collect::<Vec<_>>();

        let amount = calls.len();

        let tx = self.0.calls().force_batch(calls).await?;
        let mut res = Vec::with_capacity(amount);

        for event in tx.wait_for_success().await?.iter() {
            match event?.as_gear()? {
                Event::Gear(gear::gear::Event::MessageQueued {
                    id,
                    destination,
                    entry: MessageEntry::Init,
                    ..
                }) => res.push(Ok((id, destination))),
                Event::Utility(gear::utility::Event::ItemFailed { error }) => {
                    res.push(Err(self.0.api().decode_error(error).into()))
                }
                _ => (),
            }
        }

        if res.len() == amount {
            Ok((res, tx.block_hash()))
        } else {
            Err(Error::IncompleteBatchResult {
                expected: amount,
                found: res.len(),
            })
        }
    }

    /// Same as [`create_program_bytes`](Self::create_program_bytes), but
    /// initializes a newly created program with an encoded `payload`.
    ///
    /// # See also
    ///
    /// - [`upload_code`](Self::upload_code) function uploads a code and returns
    ///   its identifier.
    /// - [`upload_program`](Self::upload_program) function uploads a new
    ///   program and initialize it.
    pub async fn create_program(
        &self,
        code_id: CodeId,
        salt: impl Into<Vec<u8>>,
        payload: impl Encode,
        gas_limit: u64,
        value: u128,
    ) -> Result<(MessageId, ActorId, H256)> {
        self.create_program_bytes(code_id, salt, payload.encode(), gas_limit, value)
            .await
    }

    /// `pallet_gear::claim_value`
    pub async fn claim_value(&self, message_id: MessageId) -> Result<TxInBlock> {
        self.0
            .run_tx(gear::tx().gear().claim_value(message_id))
            .await
    }

    /// `pallet_gear::send_message`
    pub async fn send_message(
        &self,
        destination: ActorId,
        payload: Vec<u8>,
        gas_limit: u64,
        value: u128,
    ) -> Result<TxInBlock> {
        self.0
            .run_tx(
                gear::tx()
                    .gear()
                    .send_message(destination, payload, gas_limit, value, false),
            )
            .await
    }

    /// `pallet_gear::send_reply`
    pub async fn send_reply(
        &self,
        reply_to_id: MessageId,
        payload: Vec<u8>,
        gas_limit: u64,
        value: u128,
    ) -> Result<TxInBlock> {
        self.0
            .run_tx(
                gear::tx()
                    .gear()
                    .send_reply(reply_to_id, payload, gas_limit, value, false),
            )
            .await
    }

    /// `pallet_gear::upload_code`
    pub async fn upload_code(&self, code: Vec<u8>) -> Result<TxInBlock> {
        self.0.run_tx(gear::tx().gear().upload_code(code)).await
    }

    /// `pallet_gear::upload_program`
    pub async fn upload_program(
        &self,
        code: Vec<u8>,
        salt: Vec<u8>,
        payload: Vec<u8>,
        gas_limit: u64,
        value: u128,
    ) -> Result<TxInBlock> {
        self.0
            .run_tx(
                gear::tx()
                    .gear()
                    .upload_program(code, salt, payload, gas_limit, value, false),
            )
            .await
    }
}

impl SignerCalls<'_> {
    /// Sends the `pallet-gear-eth-bridge::reset_overflowed_queue` extrinsic.
    ///
    /// This function returns a hash of the block with the transaction.
    pub async fn reset_overflowed_queue(&self, encoded_finality_proof: Vec<u8>) -> Result<H256> {
        let tx = self
            .0
            .run_tx(
                gear::tx()
                    .gear_eth_bridge()
                    .reset_overflowed_queue(encoded_finality_proof),
            )
            .await?;

        for event in tx.wait_for_success().await?.iter() {
            if let Event::GearEthBridge(gear::gear_eth_bridge::Event::QueueReset) =
                event?.as_gear()?
            {
                return Ok(tx.block_hash());
            }
        }

        Err(Error::EventNotFound)
    }
}

// pallet-utility
impl SignerCalls<'_> {
    /// `pallet_utility::force_batch`
    pub async fn force_batch(&self, calls: Vec<RuntimeCall>) -> Result<TxInBlock> {
        self.0.run_tx(gear::tx().utility().force_batch(calls)).await
    }
}

// pallet-sudo
impl SignerCalls<'_> {
    /// `pallet_sudo::sudo_unchecked_weight`
    pub async fn sudo_unchecked_weight(&self, call: RuntimeCall, weight: Weight) -> EventsResult {
        self.0
            .sudo_run_tx(gear::tx().sudo().sudo_unchecked_weight(call, weight))
            .await
    }
}

// pallet-gear-voucher
impl SignerCalls<'_> {
    /// `pallet_gear_voucher::issue`
    pub async fn issue_voucher(
        &self,
        spender: impl IntoAccountId32,
        balance: u128,
        programs: Option<Vec<ActorId>>,
        code_uploading: bool,
        duration: u32,
    ) -> Result<TxInBlock> {
        self.0
            .run_tx(gear::tx().gear_voucher().issue(
                spender.into_account_id(),
                balance,
                programs,
                code_uploading,
                duration,
            ))
            .await
    }

    /// `pallet_gear_voucher::update`
    #[allow(clippy::too_many_arguments)]
    pub async fn update_voucher(
        &self,
        spender: impl IntoAccountId32,
        voucher_id: VoucherId,
        move_ownership: Option<impl IntoAccountId32>,
        balance_top_up: Option<u128>,
        append_programs: Option<Option<Vec<ActorId>>>,
        code_uploading: Option<bool>,
        prolong_duration: u32,
    ) -> Result<TxInBlock> {
        self.0
            .run_tx(gear::tx().gear_voucher().update(
                spender.into_account_id(),
                voucher_id,
                move_ownership.map(|id| id.into_account_id()),
                balance_top_up,
                append_programs,
                code_uploading,
                Some(prolong_duration),
            ))
            .await
    }

    /// `pallet_gear_voucher::revoke`
    pub async fn revoke_voucher(
        &self,
        spender: impl IntoAccountId32,
        voucher_id: VoucherId,
    ) -> Result<TxInBlock> {
        self.0
            .run_tx(
                gear::tx()
                    .gear_voucher()
                    .revoke(spender.into_account_id(), voucher_id),
            )
            .await
    }

    /// `pallet_gear_voucher::decline`
    pub async fn decline_voucher(&self, voucher_id: VoucherId) -> Result<TxInBlock> {
        self.0
            .run_tx(gear::tx().gear_voucher().decline(voucher_id))
            .await
    }

    /// `pallet_gear_voucher::call`
    pub async fn upload_code_with_voucher(
        &self,
        voucher_id: VoucherId,
        code: Vec<u8>,
    ) -> Result<TxInBlock> {
        let call = PrepaidCall::<u128>::UploadCode { code };

        self.0
            .run_tx(gear::tx().gear_voucher().call(voucher_id, call))
            .await
    }

    /// `pallet_gear_voucher::call`
    pub async fn send_message_with_voucher(
        &self,
        voucher_id: VoucherId,
        destination: ActorId,
        payload: Vec<u8>,
        gas_limit: u64,
        value: u128,
        keep_alive: bool,
    ) -> Result<TxInBlock> {
        let call = PrepaidCall::<u128>::SendMessage {
            destination,
            payload,
            gas_limit,
            value,
            keep_alive,
        };

        self.0
            .run_tx(gear::tx().gear_voucher().call(voucher_id, call))
            .await
    }

    /// `pallet_gear_voucher::call`
    pub async fn send_reply_with_voucher(
        &self,
        voucher_id: VoucherId,
        reply_to_id: MessageId,
        payload: Vec<u8>,
        gas_limit: u64,
        value: u128,
        keep_alive: bool,
    ) -> Result<TxInBlock> {
        let call = PrepaidCall::<u128>::SendReply {
            reply_to_id,
            payload,
            gas_limit,
            value,
            keep_alive,
        };

        self.0
            .run_tx(gear::tx().gear_voucher().call(voucher_id, call))
            .await
    }

    /// `pallet_gear_voucher::call`
    pub async fn decline_voucher_with_voucher(&self, voucher_id: VoucherId) -> Result<TxInBlock> {
        let call = PrepaidCall::<u128>::DeclineVoucher;

        self.0
            .run_tx(gear::tx().gear_voucher().call(voucher_id, call))
            .await
    }
}
