// This file is part of Gear.

// Copyright (C) 2022-2025 Gear Technologies Inc.
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

use super::{GearApi, Result};
use crate::Error;
use gear_core::ids::ActorId;
use gsdk::metadata::{
    Event,
    runtime_types::pallet_gear_voucher::{internal::VoucherId, pallet::Event as VoucherEvent},
};
use subxt::utils::H256;

impl GearApi {
    /// Issue a new voucher.
    ///
    /// Returns issued `voucher_id` at specified `at_block_hash`.
    ///
    /// Arguments:
    /// * spender:  user id that is eligible to use the voucher;
    /// * balance:  voucher balance could be used for transactions fees and gas;
    /// * programs: pool of programs spender can interact with, if None - means
    ///   any program, limited by Config param;
    /// * code_uploading: allow voucher to be used as payer for `upload_code`
    ///   transactions fee;
    /// * duration: amount of blocks voucher could be used by spender and
    ///   couldn't be revoked by owner. Must be out in [MinDuration;
    ///   MaxDuration] constants. Expiration block of the voucher calculates as:
    ///   current bn (extrinsic exec bn) + duration + 1.
    pub async fn issue_voucher(
        &self,
        spender: ActorId,
        balance: u128,
        programs: Option<Vec<ActorId>>,
        code_uploading: bool,
        duration: u32,
    ) -> Result<(VoucherId, H256)> {
        let spender: [u8; 32] = spender.into();

        let tx = self
            .0
            .calls
            .issue_voucher(spender, balance, programs, code_uploading, duration)
            .await?;

        for event in tx.wait_for_success().await?.iter() {
            if let Event::GearVoucher(VoucherEvent::VoucherIssued { voucher_id, .. }) =
                event?.as_root_event::<Event>()?
            {
                return Ok((voucher_id, tx.block_hash()));
            }
        }

        Err(Error::EventNotFound)
    }

    /// Update existing voucher.
    ///
    /// This extrinsic updates existing voucher: it can only extend vouchers
    /// rights in terms of balance, validity or programs to interact pool.
    ///
    /// Can only be called by the voucher owner.
    ///
    /// Arguments:
    /// * spender:          account id of the voucher spender;
    /// * voucher_id:       voucher id to be updated;
    /// * move_ownership:   optionally moves ownership to another account;
    /// * balance_top_up:   optionally top ups balance of the voucher from
    ///   origins balance;
    /// * append_programs:  optionally extends pool of programs by
    ///   `Some(programs_set)` passed or allows it to interact with any program
    ///   by `None` passed;
    /// * code_uploading:   optionally allows voucher to be used to pay fees for
    ///   `upload_code` extrinsics;
    /// * prolong_duration: optionally increases expiry block number. If voucher
    ///   is expired, prolongs since current bn. Validity prolongation (since
    ///   current block number for expired or since storage written expiry)
    ///   should be in [MinDuration; MaxDuration], in other words voucher
    ///   couldn't have expiry greater than current block number + MaxDuration.
    #[allow(clippy::too_many_arguments)]
    pub async fn update_voucher(
        &self,
        spender: ActorId,
        voucher_id: VoucherId,
        move_ownership: Option<ActorId>,
        balance_top_up: Option<u128>,
        append_programs: Option<Option<Vec<ActorId>>>,
        code_uploading: Option<bool>,
        prolong_duration: u32,
    ) -> Result<(VoucherId, H256)> {
        let spender: [u8; 32] = spender.into();
        let move_ownership: Option<[u8; 32]> = move_ownership.map(|v| v.into());

        let tx = self
            .0
            .calls
            .update_voucher(
                spender,
                voucher_id,
                move_ownership,
                balance_top_up,
                append_programs,
                code_uploading,
                prolong_duration,
            )
            .await?;

        for event in tx.wait_for_success().await?.iter() {
            if let Event::GearVoucher(VoucherEvent::VoucherUpdated { voucher_id, .. }) =
                event?.as_root_event::<Event>()?
            {
                return Ok((voucher_id, tx.block_hash()));
            }
        }

        Err(Error::EventNotFound)
    }

    /// Revoke existing voucher.
    ///
    /// This extrinsic revokes existing voucher, if current block is greater
    /// than expiration block of the voucher (it is no longer valid).
    ///
    /// Currently it means sending of all balance from voucher account to
    /// voucher owner without voucher removal from storage map, but this
    /// behavior may change in future, as well as the origin validation:
    /// only owner is able to revoke voucher now.
    ///
    /// Arguments:
    /// * spender:    account id of the voucher spender;
    /// * voucher_id: voucher id to be revoked.
    pub async fn revoke_voucher(
        &self,
        spender: ActorId,
        voucher_id: VoucherId,
    ) -> Result<(VoucherId, H256)> {
        let spender: [u8; 32] = spender.into();

        let tx = self.0.calls.revoke_voucher(spender, voucher_id).await?;

        for event in tx.wait_for_success().await?.iter() {
            if let Event::GearVoucher(VoucherEvent::VoucherRevoked { voucher_id, .. }) =
                event?.as_root_event::<Event>()?
            {
                return Ok((voucher_id, tx.block_hash()));
            }
        }

        Err(Error::EventNotFound)
    }

    /// Decline existing and not expired voucher.
    ///
    /// This extrinsic expires voucher of the caller, if it's still active,
    /// allowing it to be revoked.
    ///
    /// Arguments:
    /// * voucher_id:   voucher id to be declined.
    pub async fn decline_voucher(&self, voucher_id: VoucherId) -> Result<(VoucherId, H256)> {
        let tx = self.0.calls.decline_voucher(voucher_id).await?;

        for event in tx.wait_for_success().await?.iter() {
            if let Event::GearVoucher(VoucherEvent::VoucherDeclined { voucher_id, .. }) =
                event?.as_root_event::<Event>()?
            {
                return Ok((voucher_id, tx.block_hash()));
            }
        }

        Err(Error::EventNotFound)
    }

    /// Decline existing and not expired voucher with voucher.
    pub async fn decline_voucher_with_voucher(
        &self,
        voucher_id: VoucherId,
    ) -> Result<(VoucherId, H256)> {
        let tx = self
            .0
            .calls
            .decline_voucher_with_voucher(voucher_id)
            .await?;

        for event in tx.wait_for_success().await?.iter() {
            if let Event::GearVoucher(VoucherEvent::VoucherDeclined { voucher_id, .. }) =
                event?.as_root_event::<Event>()?
            {
                return Ok((voucher_id, tx.block_hash()));
            }
        }

        Err(Error::EventNotFound)
    }
}
