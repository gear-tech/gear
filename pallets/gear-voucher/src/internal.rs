// This file is part of Gear.

// Copyright (C) 2021-2024 Gear Technologies Inc.
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
use gear_core::{declare_id, ids};
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
    ///                            call can be determined;
    ///     * For messaging calls: The voucher destinations limitations accept
    ///                            determined destination;
    ///     * For codes uploading: The voucher allows code uploading.
    ///
    /// * Call is [`Self::call_deprecated`]:
    ///     * For messaging calls: The destination program of the given prepaid
    ///                            call can be determined.
    ///     * For codes uploading: NEVER.
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

            #[allow(deprecated)]
            Self::call_deprecated { call: prepaid_call }
                if !matches!(prepaid_call, PrepaidCall::UploadCode { .. }) =>
            {
                Pallet::<T>::call_deprecated_sponsor(&caller, prepaid_call)
            }

            _ => None,
        }
    }
}

impl<T: Config> Pallet<T> {
    /// Return the account id of a synthetical account used to sponsor gas
    /// and transaction fee for legacy vouchers implementation.
    #[deprecated = "Legacy voucher issuing logic is deprecated, and this and \
    `call_deprecated` extrinsic exist only for backward support"]
    pub fn call_deprecated_sponsor(
        who: &T::AccountId,
        call: &PrepaidCall<BalanceOf<T>>,
    ) -> Option<T::AccountId> {
        if matches!(call, PrepaidCall::UploadCode { .. }) {
            return None;
        };

        #[allow(deprecated)]
        Self::prepaid_call_destination(who, call).map(|program_id| {
            let entropy = (b"modlpy/voucher__", who, program_id).using_encoded(blake2_256);
            Decode::decode(&mut TrailingZeroInput::new(entropy.as_ref()))
                .expect("infinite length input; no invalid inputs for type; qed")
        })
    }

    /// Validate prepaid call with related params of voucher: origin, expiration.
    pub fn validate_prepaid(
        origin: AccountIdOf<T>,
        voucher_id: VoucherId,
        call: &PrepaidCall<BalanceOf<T>>,
    ) -> Result<(), Error<T>> {
        let voucher =
            Vouchers::<T>::get(origin.clone(), voucher_id).ok_or(Error::<T>::InexistentVoucher)?;

        ensure!(
            <frame_system::Pallet<T>>::block_number() < voucher.expiry,
            Error::<T>::VoucherExpired
        );

        match call {
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
    ) -> Option<ProgramId> {
        match call {
            PrepaidCall::SendMessage { destination, .. } => Some(*destination),
            PrepaidCall::SendReply { reply_to_id, .. } => {
                T::Mailbox::peek(who, reply_to_id).map(|stored_message| stored_message.source())
            }
            PrepaidCall::UploadCode { .. } => None,
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
        call: PrepaidCall<Self::Balance>,
    ) -> DispatchResultWithPostInfo;
}

declare_id!(VoucherId: "Voucher identifier");

impl VoucherId {
    pub fn generate<T: Config>() -> Self {
        const SALT: &[u8] = b"voucher";

        CounterImpl::<u64, IssuedWrap<T>>::increase();
        let nonce = CounterImpl::<u64, IssuedWrap<T>>::get();

        let argument = [SALT, &nonce.to_le_bytes()].concat();
        ids::hash(&argument).into()
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
    pub programs: Option<BTreeSet<ProgramId>>,
    /// Flag if this voucher's covers uploading codes as prepaid call.
    pub code_uploading: bool,
    /// The block number at and after which voucher couldn't be used and
    /// can be revoked by owner.
    pub expiry: BlockNumber,
}

impl<AccountId, BlockNumber> VoucherInfo<AccountId, BlockNumber> {
    pub fn contains(&self, program_id: ProgramId) -> bool {
        self.programs
            .as_ref()
            .map_or(true, |v| v.contains(&program_id))
    }
}

/// Prepaid call to be executed on-chain.
#[derive(Debug, Clone, Encode, Decode, TypeInfo, PartialEq, Eq, PartialOrd, Ord)]
pub enum PrepaidCall<Balance> {
    SendMessage {
        destination: ProgramId,
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
    // TODO (breathx): add processing for it [DONE]
    // TODO (breathx): add bool flag for voucher [DONE]
    // TODO (breathx): add bool to `Pallet::issue` and `Pallet::update` [DONE]
    // TODO (breathx): add validation for call from voucher: `voucher.whitelists(&prepaid_call)` [DONE]
    // TODO (breathx): forbid for `Pallet::call_deprecated` [DONE]
    // TODO (breathx): tests for:
    //                  * `Pallet::update()`: ok, err, noop;
    //                  * `Pallet::call_deprecated`: forbidden (result + fees);
    //                  * `Pallet::call`: eligible (result + fees), non-eligible (result + fees).
    UploadCode {
        code: Vec<u8>,
    },
}
