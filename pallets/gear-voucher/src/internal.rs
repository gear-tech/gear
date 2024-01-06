// This file is part of Gear.

// Copyright (C) 2021-2023 Gear Technologies Inc.
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
    pub fn is_legit(&self, who: AccountIdOf<T>) -> bool {
        match self {
            Self::call { voucher_id, call } => {
                Pallet::<T>::validate_prepaid(who, *voucher_id, call).is_ok()
            }
            _ => true,
        }
    }

    // NOTE: delete [`None`] return once fn `GearVoucher::call_deprecated()` is removed.
    pub fn sponsored_by(&self, who: AccountIdOf<T>) -> Option<AccountIdOf<T>> {
        match self {
            #[allow(deprecated)]
            Self::call_deprecated { call } => Pallet::<T>::sponsor_of(&who, call),
            Self::call { voucher_id, call } => Pallet::<T>::validate_prepaid(who, *voucher_id, call)
                .map(|_| (*voucher_id).cast()).map_err(|_| {
                    // This place may be considered as unreachable due to checks in `SignedExtension`.
                    log::error!("Signed extension of voucher validity passed invalid voucher for fee delegation");
                }).ok(),
            _ => None,
        }
    }
}

#[deprecated = "Relates to legacy voucher logic and `call_deprecated`"]
impl<T: Config> Pallet<T> {
    /// Return synthesized account ID based on call data.
    pub fn sponsor_of(
        who: &T::AccountId,
        call: &PrepaidCall<BalanceOf<T>>,
    ) -> Option<T::AccountId> {
        #[allow(deprecated)]
        match call {
            PrepaidCall::SendMessage { destination, .. } => {
                Some(Self::voucher_id(who, destination))
            }
            PrepaidCall::SendReply { reply_to_id, .. } => T::Mailbox::peek(who, reply_to_id)
                .map(|stored_message| Self::voucher_id(who, &stored_message.source())),
        }
    }

    /// Derive a synthesized account ID from an account ID and a program ID.
    pub fn voucher_id(who: &T::AccountId, program_id: &ProgramId) -> T::AccountId {
        let entropy = (b"modlpy/voucher__", who, program_id).using_encoded(blake2_256);
        Decode::decode(&mut TrailingZeroInput::new(entropy.as_ref()))
            .expect("infinite length input; no invalid inputs for type; qed")
    }
}

impl<T: Config> Pallet<T> {
    /// Validate prepaid call with params of voucher:
    /// origin, expiration and call relation.
    pub fn validate_prepaid(
        origin: AccountIdOf<T>,
        voucher_id: VoucherId,
        call: &PrepaidCall<BalanceOf<T>>,
    ) -> Result<(), Error<T>> {
        let voucher =
            Vouchers::<T>::get(origin.clone(), voucher_id).ok_or(Error::<T>::InexistentVoucher)?;

        ensure!(
            <frame_system::Pallet<T>>::block_number() <= voucher.validity,
            Error::<T>::VoucherExpired
        );

        if let Some(ref programs) = voucher.programs {
            let destination =
                Self::destination_program(&origin, call).ok_or(Error::<T>::UnknownDestination)?;

            ensure!(
                programs.contains(&destination),
                Error::<T>::InappropriateDestination
            );
        }

        Ok(())
    }

    /// Return destination program of the [`PrepaidCall`].
    pub fn destination_program(
        who: &T::AccountId,
        call: &PrepaidCall<BalanceOf<T>>,
    ) -> Option<ProgramId> {
        match call {
            PrepaidCall::SendMessage { destination, .. } => Some(*destination),
            PrepaidCall::SendReply { reply_to_id, .. } => {
                T::Mailbox::peek(who, reply_to_id).map(|stored_message| stored_message.source())
            }
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
    /// Block number since voucher couldn't be used (able to be revoked by owner).
    pub validity: BlockNumber,
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
}
