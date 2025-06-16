// This file is part of Gear.

// Copyright (C) 2021-2025 Gear Technologies Inc.
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

use crate::*;
use common::{
    storage::{Counter, CounterImpl, Mailbox},
    Origin,
};
use frame_system::pallet_prelude::BlockNumberFor;
use gear_core::ids;
use sp_std::collections::btree_set::BTreeSet;

impl<T: Config> crate::Call<T>
where
    T::AccountId: Origin,
{
    /// Returns account id that pays for gas purchase and transaction fee
    /// for processing this ['pallet_gear_voucher::Call'], if:
    ///
    /// * Call is [`Self::call`]:
    ///     * Voucher with the given voucher id exists;
    ///     * Caller is eligible to use the voucher;
    ///     * The voucher is not expired;
    ///     * For messaging calls: The destination program of the given prepaid
    ///       call can be determined;
    ///     * For messaging calls: The voucher destinations limitations accept
    ///       determined destination;
    ///     * For codes uploading: The voucher allows code uploading.
    ///
    /// Returns [`None`] for other cases.
    pub fn get_sponsor(&self, caller: AccountIdOf<T>) -> Option<AccountIdOf<T>> {
        match self {
            Self::call {
                voucher_id,
                call: prepaid_call,
            } => Pallet::<T>::validate_prepaid(caller, *voucher_id, prepaid_call)
                .map(|_| (*voucher_id).cast())
                .ok(),

            _ => None,
        }
    }
}

impl<T: Config> Pallet<T> {
    /// Queries a voucher and asserts its validity.
    pub fn get_active_voucher(
        origin: AccountIdOf<T>,
        voucher_id: VoucherId,
    ) -> Result<VoucherInfo<AccountIdOf<T>, BlockNumberFor<T>>, Error<T>> {
        let voucher =
            Vouchers::<T>::get(origin.clone(), voucher_id).ok_or(Error::<T>::InexistentVoucher)?;

        ensure!(
            <frame_system::Pallet<T>>::block_number() < voucher.expiry,
            Error::<T>::VoucherExpired
        );

        Ok(voucher)
    }

    /// Validate prepaid call with related params of voucher: origin, expiration.
    pub fn validate_prepaid(
        origin: AccountIdOf<T>,
        voucher_id: VoucherId,
        call: &PrepaidCall<BalanceOf<T>>,
    ) -> Result<(), Error<T>> {
        let voucher = Self::get_active_voucher(origin.clone(), voucher_id)?;

        match call {
            PrepaidCall::DeclineVoucher => (),
            PrepaidCall::UploadCode { .. } => {
                ensure!(voucher.code_uploading, Error::<T>::CodeUploadingDisabled)
            }
            PrepaidCall::SendMessage { .. } | PrepaidCall::SendReply { .. } => {
                if let Some(ref programs) = voucher.programs {
                    let destination = Self::prepaid_call_destination(&origin, call)
                        .ok_or(Error::<T>::UnknownDestination)?;

                    ensure!(
                        programs.contains(&destination),
                        Error::<T>::InappropriateDestination
                    );
                }
            }
        }

        Ok(())
    }

    /// Return destination program of the [`PrepaidCall`], if exists.
    pub fn prepaid_call_destination(
        who: &T::AccountId,
        call: &PrepaidCall<BalanceOf<T>>,
    ) -> Option<ActorId> {
        match call {
            PrepaidCall::SendMessage { destination, .. } => Some(*destination),
            PrepaidCall::SendReply { reply_to_id, .. } => {
                T::Mailbox::peek(who, reply_to_id).map(|stored_message| stored_message.source())
            }
            PrepaidCall::UploadCode { .. } | PrepaidCall::DeclineVoucher => None,
        }
    }
}

/// Trait for processing prepaid calls by any implementor.
pub trait PrepaidCallsDispatcher {
    type AccountId;
    type Balance;

    /// Returns weight of processing for call.
    fn weight(call: &PrepaidCall<Self::Balance>) -> Weight;

    /// Processes prepaid call with specific sponsor from origins address.
    fn dispatch(
        account_id: Self::AccountId,
        sponsor_id: Self::AccountId,
        voucher_id: VoucherId,
        call: PrepaidCall<Self::Balance>,
    ) -> DispatchResultWithPostInfo;
}

/// Voucher identifier.
#[derive(
    Debug,
    Clone,
    Copy,
    Default,
    Hash,
    Ord,
    PartialEq,
    PartialOrd,
    Eq,
    derive_more::From,
    derive_more::AsRef,
    TypeInfo,
    Encode,
    Decode,
    MaxEncodedLen,
)]
pub struct VoucherId([u8; 32]);

impl VoucherId {
    pub fn generate<T: Config>() -> Self {
        const SALT: &[u8] = b"voucher";

        CounterImpl::<u64, IssuedWrap<T>>::increase();
        let nonce = CounterImpl::<u64, IssuedWrap<T>>::get();

        ids::hash_of_array([SALT, &nonce.to_le_bytes()]).into()
    }
}

impl Origin for VoucherId {
    fn into_origin(self) -> H256 {
        self.0.into()
    }

    fn from_origin(val: H256) -> Self {
        Self(val.to_fixed_bytes())
    }
}

/// Type containing all data about voucher.
#[derive(Debug, Encode, Decode, TypeInfo)]
pub struct VoucherInfo<AccountId, BlockNumber> {
    /// Owner of the voucher.
    /// May be different to original issuer.
    /// Owner manages and claims back remaining balance of the voucher.
    pub owner: AccountId,
    /// Set of programs this voucher could be used to interact with.
    /// In case of [`None`] means any gear program.
    pub programs: Option<BTreeSet<ActorId>>,
    /// Flag if this voucher's covers uploading codes as prepaid call.
    pub code_uploading: bool,
    /// The block number at and after which voucher couldn't be used and
    /// can be revoked by owner.
    pub expiry: BlockNumber,
}

impl<AccountId, BlockNumber> VoucherInfo<AccountId, BlockNumber> {
    pub fn contains(&self, program_id: ActorId) -> bool {
        self.programs
            .as_ref()
            .is_none_or(|v| v.contains(&program_id))
    }
}

/// Prepaid call to be executed on-chain.
#[derive(Debug, Clone, Encode, Decode, TypeInfo, PartialEq, Eq, PartialOrd, Ord)]
pub enum PrepaidCall<Balance> {
    SendMessage {
        destination: ActorId,
        payload: Vec<u8>,
        gas_limit: u64,
        value: Balance,
        keep_alive: bool,
    },
    SendReply {
        reply_to_id: MessageId,
        payload: Vec<u8>,
        gas_limit: u64,
        value: Balance,
        keep_alive: bool,
    },
    UploadCode {
        code: Vec<u8>,
    },
    DeclineVoucher,
}
