// This file is part of Gear.

// Copyright (C) 2022-2024 Gear Technologies Inc.
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
use gear_core::ids::ProgramId;
use gsdk::{
    ext::sp_core::H256,
    metadata::{
        runtime_types::pallet_gear_voucher::{
            internal::{VoucherId, VoucherPermissions, VoucherPermissionsExtend},
            pallet::Event as VoucherEvent,
        },
        Event,
    },
};

impl GearApi {
    /// Issue a new voucher.
    ///
    /// See [`pallet_gear_voucher::pallet::Call::issue`]
    ///
    /// Returns issued `voucher_id` at specified `at_block_hash`.
    pub async fn issue_voucher(
        &self,
        spender: ProgramId,
        balance: u128,
        duration: u32,
        permissions: VoucherPermissions,
    ) -> Result<(VoucherId, H256)> {
        let spender: [u8; 32] = spender.into();

        let tx = self
            .0
            .calls
            .issue_voucher(spender, balance, duration, permissions)
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
    /// See [`pallet_gear_voucher::pallet::Call::update`]
    ///
    /// Can only be called by the voucher owner.
    #[allow(clippy::too_many_arguments)]
    pub async fn update_voucher(
        &self,
        spender: ProgramId,
        voucher_id: VoucherId,
        move_ownership: Option<ProgramId>,
        balance_top_up: Option<u128>,
        prolong_duration: u32,
        permissions_extnend: VoucherPermissionsExtend,
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
                prolong_duration,
                permissions_extnend,
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
        spender: ProgramId,
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
